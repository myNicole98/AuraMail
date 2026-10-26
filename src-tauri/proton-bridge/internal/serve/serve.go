// Package serve runs the loopback IMAP and SMTP bridge for every stored
// Proton account.
package serve

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"syscall"

	"codeberg.org/emersion/hydroxide/auth"
	"codeberg.org/emersion/hydroxide/config"
	"codeberg.org/emersion/hydroxide/events"
	hydroxideimap "codeberg.org/emersion/hydroxide/imap"
	"codeberg.org/emersion/hydroxide/internal/realauth"
	"codeberg.org/emersion/hydroxide/internal/status"
	"codeberg.org/emersion/hydroxide/protonmail"
	hydroxidesmtp "codeberg.org/emersion/hydroxide/smtp"

	imapserver "github.com/emersion/go-imap/server"
	smtpserver "github.com/emersion/go-smtp"
)

const maxCommandLine = 1 << 20

// Config is the serve-mode configuration.
type Config struct {
	IMAPPort  int
	SMTPPort  int
	ConfigDir string

	// Stdin and Stdout default to the process standard streams.
	Stdin  io.Reader
	Stdout io.Writer

	// APIEndpoint overrides Proton's production API root.
	APIEndpoint string

	listen func(network, addr string) (net.Listener, error)
}

type readyEvent struct {
	Event    string `json:"event"`
	IMAPPort int    `json:"imap_port"`
	SMTPPort int    `json:"smtp_port"`
}

// Run binds the loopback IMAP and SMTP listeners, prints one ready line, and
// blocks until the context is canceled, stdin closes, or a shutdown command is
// received.
func Run(ctx context.Context, cfg Config) error {
	configDir := cfg.ConfigDir
	if configDir == "" {
		authPath, err := config.Path("auth.json")
		if err != nil {
			return fmt.Errorf("serve: cannot locate the bridge config directory: %w", err)
		}
		configDir = filepath.Dir(authPath)
	}

	lock, err := acquireLock(configDir)
	if err != nil {
		return err
	}
	defer lock.Close()

	stdin := cfg.Stdin
	if stdin == nil {
		stdin = os.Stdin
	}
	stdout := cfg.Stdout
	if stdout == nil {
		stdout = os.Stdout
	}
	listen := cfg.listen
	if listen == nil {
		listen = net.Listen
	}
	endpoint := cfg.APIEndpoint
	if endpoint == "" {
		endpoint = realauth.DefaultAPIEndpoint
	}

	lines := &lineWriter{w: stdout}
	emitter := status.NewEmitter(func(ev status.Event) {
		_ = lines.write(ev)
	})

	newClient := func(username string) *protonmail.Client {
		return &protonmail.Client{
			RootURL:    endpoint,
			AppVersion: realauth.DefaultAppVersion,
			HTTPClient: &http.Client{
				Transport: &status.Transport{
					Username: username,
					Emitter:  emitter,
				},
			},
		}
	}

	sessions := auth.NewManager(newClient)
	eventsManager := events.NewManager()

	imapAddr := loopbackAddr(cfg.IMAPPort)
	imapListener, err := listen("tcp", imapAddr)
	if err != nil {
		return fmt.Errorf("serve: cannot listen for IMAP on %s: %w", imapAddr, err)
	}

	smtpAddr := loopbackAddr(cfg.SMTPPort)
	smtpListener, err := listen("tcp", smtpAddr)
	if err != nil {
		imapListener.Close()
		return fmt.Errorf("serve: cannot listen for SMTP on %s: %w", smtpAddr, err)
	}

	imapServer := imapserver.New(hydroxideimap.New(sessions, eventsManager))
	imapServer.AllowInsecureAuth = true
	smtpServer := smtpserver.NewServer(hydroxidesmtp.New(sessions))
	smtpServer.AllowInsecureAuth = true

	imapPort, err := listenerPort(imapListener)
	if err != nil {
		imapListener.Close()
		smtpListener.Close()
		return fmt.Errorf("serve: cannot read the IMAP listener address: %w", err)
	}
	smtpPort, err := listenerPort(smtpListener)
	if err != nil {
		imapListener.Close()
		smtpListener.Close()
		return fmt.Errorf("serve: cannot read the SMTP listener address: %w", err)
	}

	serverErrors := make(chan error, 2)
	go func() {
		serverErrors <- imapServer.Serve(imapListener)
	}()
	go func() {
		serverErrors <- smtpServer.Serve(smtpListener)
	}()

	if err := lines.write(readyEvent{Event: "ready", IMAPPort: imapPort, SMTPPort: smtpPort}); err != nil {
		closeServers(imapListener, smtpListener, imapServer, smtpServer)
		return fmt.Errorf("serve: cannot write the ready line: %w", err)
	}

	stop := make(chan struct{})
	go readShutdown(stdin, stop)

	var runErr error
	select {
	case <-ctx.Done():
	case <-stop:
	case err := <-serverErrors:
		if err != nil && !errors.Is(err, net.ErrClosed) {
			runErr = fmt.Errorf("serve: IMAP/SMTP server failed: %w", err)
		}
	}

	closeServers(imapListener, smtpListener, imapServer, smtpServer)
	return runErr
}

func loopbackAddr(port int) string {
	return net.JoinHostPort("127.0.0.1", strconv.Itoa(port))
}

func listenerPort(listener net.Listener) (int, error) {
	addr, ok := listener.Addr().(*net.TCPAddr)
	if !ok {
		return 0, fmt.Errorf("unexpected address %q", listener.Addr())
	}
	return addr.Port, nil
}

func closeServers(imapListener, smtpListener net.Listener, imapServer *imapserver.Server, smtpServer *smtpserver.Server) {
	imapListener.Close()
	smtpListener.Close()
	imapServer.Close()
	smtpServer.Close()
}

// readShutdown closes stop on a shutdown command, stdin EOF, or a read error.
func readShutdown(r io.Reader, stop chan<- struct{}) {
	defer close(stop)

	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 4096), maxCommandLine)
	for scanner.Scan() {
		var command struct {
			Op string `json:"op"`
		}
		if err := json.Unmarshal(scanner.Bytes(), &command); err != nil {
			continue
		}
		if command.Op == "shutdown" {
			return
		}
	}
}

type lineWriter struct {
	mu sync.Mutex
	w  io.Writer
}

func (w *lineWriter) write(value any) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	return json.NewEncoder(w.w).Encode(value)
}

func acquireLock(dir string) (*os.File, error) {
	if err := os.MkdirAll(dir, 0700); err != nil {
		return nil, fmt.Errorf("serve: cannot create the bridge config directory %s: %w", dir, err)
	}
	// MkdirAll does not tighten an existing directory.
	if err := os.Chmod(dir, 0700); err != nil {
		return nil, fmt.Errorf("serve: cannot secure the bridge config directory %s: %w", dir, err)
	}

	path := filepath.Join(dir, "bridge.lock")
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, fmt.Errorf("serve: cannot open the bridge lock %s: %w", path, err)
	}

	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return nil, fmt.Errorf("serve: another bridge process is already running (could not lock %s)", path)
		}
		return nil, fmt.Errorf("serve: cannot lock %s: %w", path, err)
	}

	return f, nil
}
