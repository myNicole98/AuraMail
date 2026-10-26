// Package status emits structured Proton API status events.
package status

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"sync"
)

const (
	// AppealURL is Proton's anti-abuse appeal form.
	AppealURL = "https://proton.me/support/appeal-abuse"
	// AccessLimitedMessage explains a terminal access_limited event.
	AccessLimitedMessage = "Proton temporarily limited API access for this account. Use the official Proton app and try again later."
)

// Event is the structured access_limited status line.
type Event struct {
	Event     string `json:"event"`
	Username  string `json:"username"`
	Message   string `json:"message"`
	AppealURL string `json:"appeal_url"`
}

// Emitter emits at most one access_limited event per username.
type Emitter struct {
	emit func(Event)

	mu   sync.Mutex
	seen map[string]struct{}
}

// NewEmitter returns an emitter that calls emit for the first 2028 response
// seen for each username.
func NewEmitter(emit func(Event)) *Emitter {
	return &Emitter{
		emit: emit,
		seen: make(map[string]struct{}),
	}
}

func (e *Emitter) accessLimited(username string) {
	if e == nil {
		return
	}

	e.mu.Lock()
	if _, ok := e.seen[username]; ok {
		e.mu.Unlock()
		return
	}
	e.seen[username] = struct{}{}
	emit := e.emit
	e.mu.Unlock()

	if emit != nil {
		emit(Event{
			Event:     "access_limited",
			Username:  username,
			Message:   AccessLimitedMessage,
			AppealURL: AppealURL,
		})
	}
}

// Transport wraps a Proton HTTP transport and reports access_limited errors.
type Transport struct {
	Base     http.RoundTripper
	Username string
	Emitter  *Emitter
}

// RoundTrip inspects error responses for Proton code 2028 and restores the
// response body for the caller.
func (t *Transport) RoundTrip(req *http.Request) (*http.Response, error) {
	base := t.Base
	if base == nil {
		base = http.DefaultTransport
	}

	resp, err := base.RoundTrip(req)
	if err != nil || resp == nil {
		return resp, err
	}
	if resp.StatusCode < 400 {
		return resp, nil
	}

	body, err := io.ReadAll(resp.Body)
	resp.Body.Close()
	if err != nil {
		return nil, err
	}
	resp.Body = io.NopCloser(bytes.NewReader(body))

	if isAccessLimited(body) {
		t.Emitter.accessLimited(t.Username)
	}
	return resp, nil
}

func isAccessLimited(body []byte) bool {
	var payload struct {
		Code int `json:"Code"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return false
	}
	return payload.Code == 2028
}
