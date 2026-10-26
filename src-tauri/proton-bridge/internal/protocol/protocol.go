// Package protocol defines the JSON-line contract between AuraMail and the
// sidecar's auth mode. Events never carry credentials or TOTP/CAPTCHA inputs.
package protocol

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
)

// OpLogin starts an interactive Proton login.
const OpLogin = "login"

// Event names emitted by the auth flow.
const (
	EventCaptchaRequired         = "captcha_required"
	EventTOTPRequired            = "totp_required"
	EventMailboxPasswordRequired = "mailbox_password_required"
	EventAccessLimited           = "access_limited"
	EventError                   = "error"
	EventSuccess                 = "success"
)

// Request is one decoded auth-mode stdin line.
type Request struct {
	Op               string `json:"op"`
	Username         string `json:"username,omitempty"`
	Password         string `json:"password,omitempty"`
	TOTP             string `json:"totp,omitempty"`
	MailboxPassword  string `json:"mailbox_password,omitempty"`
	CaptchaToken     string `json:"captcha_token,omitempty"`
	VerificationDone bool   `json:"verification_done,omitempty"`
}

// Credentials are the inputs needed to start a login.
type Credentials struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

// Response carries the follow-up input for a pending login.
type Response struct {
	TOTP             string `json:"totp,omitempty"`
	MailboxPassword  string `json:"mailbox_password,omitempty"`
	CaptchaToken     string `json:"captcha_token,omitempty"`
	VerificationDone bool   `json:"verification_done,omitempty"`
}

// Event is one auth-mode stdout line. It has no credential fields.
type Event struct {
	Event   string   `json:"event"`
	URL     string   `json:"url,omitempty"`
	Methods []string `json:"methods,omitempty"`
	Message string   `json:"message,omitempty"`
	// Retryable asks the caller to keep the flow open and send another response line.
	Retryable      bool   `json:"retryable,omitempty"`
	AppealURL      string `json:"appeal_url,omitempty"`
	LoginUsername  string `json:"login_username,omitempty"`
	Email          string `json:"email,omitempty"`
	DisplayName    string `json:"display_name,omitempty"`
	BridgePassword string `json:"bridge_password,omitempty"`
}

// DecodeRequest decodes one JSON line and requires a non-empty op.
func DecodeRequest(line string) (Request, error) {
	var req Request
	if err := json.Unmarshal([]byte(line), &req); err != nil {
		return Request{}, fmt.Errorf("protocol: invalid request: %w", err)
	}

	req.Op = strings.TrimSpace(req.Op)
	if req.Op == "" {
		return Request{}, errors.New("protocol: request is missing op")
	}

	return req, nil
}

// EncodeEvent writes ev as one newline-terminated JSON line.
func EncodeEvent(w io.Writer, ev Event) error {
	if strings.TrimSpace(ev.Event) == "" {
		return errors.New("protocol: event is missing an event name")
	}

	enc := json.NewEncoder(w)
	if err := enc.Encode(ev); err != nil {
		return fmt.Errorf("protocol: encode event: %w", err)
	}
	return nil
}
