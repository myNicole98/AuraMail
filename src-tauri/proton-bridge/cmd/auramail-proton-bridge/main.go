// Command auramail-proton-bridge is the AuraMail sidecar that wraps a reduced
// Hydroxide build.
package main

import (
	"fmt"
	"io"
	"os"

	"github.com/emersion/hydroxide/internal/version"
)

const usage = `usage: auramail-proton-bridge <command> [options]

Commands:
  auth      Log in to Proton and store the bridge credentials
  serve     Run the local IMAP and SMTP bridge
  logout    Remove the stored bridge credentials
  version   Print the sidecar version

Run "auramail-proton-bridge --help" for this message.
`

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
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

// runAuth is a placeholder for the JSON-line auth flow.
func runAuth(args []string, stdout, stderr io.Writer) int {
	fmt.Fprintln(stderr, "auth: not implemented yet")
	return 1
}

// runServe is a placeholder for the IMAP/SMTP serve mode.
func runServe(args []string, stdout, stderr io.Writer) int {
	fmt.Fprintln(stderr, "serve: not implemented yet")
	return 1
}

// runLogout is a placeholder for removing a stored auth entry.
func runLogout(args []string, stdout, stderr io.Writer) int {
	fmt.Fprintln(stderr, "logout: not implemented yet")
	return 1
}
