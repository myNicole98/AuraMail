// Command auramail-proton-bridge is the AuraMail sidecar that wraps a reduced
// Hydroxide build.
package main

import (
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"os"

	"codeberg.org/emersion/hydroxide/internal/authflow"
	"codeberg.org/emersion/hydroxide/internal/protocol"
	"codeberg.org/emersion/hydroxide/internal/realauth"
	"codeberg.org/emersion/hydroxide/internal/serve"
	"codeberg.org/emersion/hydroxide/internal/version"
)

const usage = `usage: auramail-proton-bridge <command> [options]

Commands:
  auth      Log in to Proton and store the bridge credentials
  serve     Run the local IMAP and SMTP bridge
  logout    Remove the stored bridge credentials
  version   Print the sidecar version

Run "auramail-proton-bridge --help" for this message.
`

const maxAuthLine = 1 << 20

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	// The sidecar protocol is stdout JSON; upstream libraries log through the
	// standard logger, so discard it process-wide.
	log.SetOutput(io.Discard)

	if len(args) == 0 {
		fmt.Fprint(stderr, usage)
		return 2
	}

	switch args[0] {
	case "-h", "--help", "help":
		fmt.Fprint(stdout, usage)
		return 0
	case "version":
		fmt.Fprintln(stdout, version.String())
		return 0
	case "auth":
		return runAuth(args[1:], stdout, stderr)
	case "serve":
		return runServe(args[1:], stdout, stderr)
	case "logout":
		return runLogout(args[1:], stdout, stderr)
	default:
		fmt.Fprintf(stderr, "unknown command %q\n\n", args[0])
		fmt.Fprint(stderr, usage)
		return 2
	}
}

// runAuth reads one login request and one follow-up response per prompt from
// stdin. Every event is written to stdout as a JSON line.
func runAuth(args []string, stdout, stderr io.Writer) int {
	return runAuthWith(args, os.Stdin, stdout, stderr, func() authflow.ProtonAuth {
		return realauth.New()
	})
}

func runAuthWith(args []string, stdin io.Reader, stdout, stderr io.Writer, newClient func() authflow.ProtonAuth) int {
	if len(args) != 0 {
		return emitAuthError(stdout, stderr, "The auth command does not accept arguments.")
	}

	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 0, 4096), maxAuthLine)

	if !scanner.Scan() {
		if err := scanner.Err(); err != nil {
			return emitAuthError(stdout, stderr, "The login request could not be read.")
		}
		return emitAuthError(stdout, stderr, "The login request is missing.")
	}

	req, err := protocol.DecodeRequest(scanner.Text())
	if err != nil {
		return emitAuthError(stdout, stderr, "The login request is invalid.")
	}
	if req.Op != protocol.OpLogin {
		return emitAuthError(stdout, stderr, "The login request is not supported.")
	}

	ctx := context.Background()
	flow := authflow.New(newClient())
	ev, _ := flow.Start(ctx, protocol.Credentials{
		Username: req.Username,
		Password: req.Password,
	})

	for {
		if !writeAuthEvent(stdout, stderr, ev) {
			return 1
		}

		switch ev.Event {
		case protocol.EventSuccess:
			return 0
		case protocol.EventAccessLimited:
			return 1
		case protocol.EventError:
			if !ev.Retryable {
				return 1
			}
		case protocol.EventCaptchaRequired, protocol.EventTOTPRequired, protocol.EventMailboxPasswordRequired:
		default:
			return 1
		}

		if !scanner.Scan() {
			if err := scanner.Err(); err != nil {
				return emitAuthError(stdout, stderr, "The sign-in input could not be read.")
			}
			return emitAuthError(stdout, stderr, "The Proton sign-in ended before it was complete.")
		}

		var resp protocol.Response
		if err := json.Unmarshal([]byte(scanner.Text()), &resp); err != nil {
			return emitAuthError(stdout, stderr, "The sign-in response is invalid.")
		}

		ev, _ = flow.Continue(ctx, resp)
	}
}

func writeAuthEvent(stdout, stderr io.Writer, ev protocol.Event) bool {
	if err := protocol.EncodeEvent(stdout, ev); err != nil {
		fmt.Fprintln(stderr, "auth: could not write the event")
		return false
	}
	return true
}

func emitAuthError(stdout, stderr io.Writer, message string) int {
	writeAuthEvent(stdout, stderr, protocol.Event{
		Event:   protocol.EventError,
		Message: message,
	})
	return 1
}

// runServe starts the loopback IMAP and SMTP bridge.
func runServe(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("serve", flag.ContinueOnError)
	flags.SetOutput(stderr)
	imapPort := flags.Int("imap-port", 1143, "loopback IMAP port")
	smtpPort := flags.Int("smtp-port", 1025, "loopback SMTP port")

	if err := flags.Parse(args); err != nil {
		return 2
	}
	if flags.NArg() != 0 {
		fmt.Fprintln(stderr, "serve: unexpected arguments")
		return 2
	}
	if *imapPort < 0 || *imapPort > 65535 || *smtpPort < 0 || *smtpPort > 65535 {
		fmt.Fprintln(stderr, "serve: ports must be between 0 and 65535")
		return 2
	}

	err := serve.Run(context.Background(), serve.Config{
		IMAPPort: *imapPort,
		SMTPPort: *smtpPort,
		Stdin:    os.Stdin,
		Stdout:   stdout,
	})
	if err != nil {
		fmt.Fprintf(stderr, "serve: %v\n", err)
		return 1
	}
	return 0
}

// runLogout is a placeholder for removing a stored auth entry.
func runLogout(args []string, stdout, stderr io.Writer) int {
	fmt.Fprintln(stderr, "logout: not implemented yet")
	return 1
}
