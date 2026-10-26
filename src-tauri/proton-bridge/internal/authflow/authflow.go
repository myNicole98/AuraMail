// Package authflow drives the interactive Proton login state machine.
//
// Events never contain Proton credentials, TOTP codes, or inbound CAPTCHA
// tokens. The returned error is non-nil exactly for an error event, which
// callers must still emit; access_limited is terminal and must not be retried.
// ctx is checked between steps because ProtonAuth has no context-aware methods.
package authflow

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"strings"

	"github.com/ProtonMail/go-crypto/openpgp"

	"github.com/emersion/hydroxide/auth"
	"github.com/emersion/hydroxide/internal/protocol"
	"github.com/emersion/hydroxide/protonmail"
)

// AppealURL is Proton's anti-abuse appeal form.
const AppealURL = "https://proton.me/support/appeal-abuse"

// AccessLimitedMessage explains a terminal access_limited event.
const AccessLimitedMessage = "Proton temporarily limited API access for this account. Do not retry automatically."

const (
	verifyBaseURL = "https://verify.proton.me/"
	captchaMethod = "captcha"
)

// ProtonAuth is the subset of protonmail.Client used by the flow.
type ProtonAuth interface {
	AuthInfo(username string) (*protonmail.AuthInfo, error)
	Auth(username, password string, info *protonmail.AuthInfo, hv *protonmail.HumanVerificationDetails) (*protonmail.Auth, error)
	AuthTOTP(code string) (string, error)
	ListKeySalts() (map[string][]byte, error)
	Unlock(auth *protonmail.Auth, salts map[string][]byte, mailboxPassword string) (openpgp.EntityList, error)
	Identity() (login, primaryEmail, displayName string, err error)
}

type state int

const (
	stateIdle state = iota
	stateCaptcha
	stateTOTP
	stateMailbox
	stateDone
)

// Flow is one serial interactive login and is not safe for concurrent use.
type Flow struct {
	client ProtonAuth
	state  state

	username      string
	loginPassword string

	info      *protonmail.AuthInfo
	challenge *protonmail.HumanVerificationDetails
	session   *protonmail.Auth
}

// New creates a login flow backed by client.
func New(client ProtonAuth) *Flow {
	return &Flow{client: client}
}

// Start begins a login and returns the next event.
func (f *Flow) Start(ctx context.Context, creds protocol.Credentials) (protocol.Event, error) {
	if f.state != stateIdle {
		return f.eventError("authentication flow already started")
	}
	if err := ctx.Err(); err != nil {
		return f.eventError("authentication cancelled")
	}
	if strings.TrimSpace(creds.Username) == "" {
		return f.eventError("username is required")
	}
	if creds.Password == "" {
		return f.eventError("password is required")
	}

	f.username = creds.Username
	f.loginPassword = creds.Password

	info, err := f.client.AuthInfo(creds.Username)
	if err != nil {
		return f.mapError(err)
	}
	if info == nil {
		return f.fail("Proton returned no authentication information")
	}
	f.info = info

	return f.authenticate(ctx, nil)
}

// Continue advances a pending flow. A missing input returns an error event but
// leaves the flow pending.
func (f *Flow) Continue(ctx context.Context, resp protocol.Response) (protocol.Event, error) {
	if err := ctx.Err(); err != nil {
		return f.eventError("authentication cancelled")
	}

	switch f.state {
	case stateCaptcha:
		return f.continueCaptcha(ctx, resp)
	case stateTOTP:
		return f.continueTOTP(resp)
	case stateMailbox:
		return f.continueMailbox(resp)
	case stateIdle:
		return f.eventError("authentication flow has not started")
	default:
		return f.eventError("authentication flow has already finished")
	}
}

func (f *Flow) authenticate(ctx context.Context, hv *protonmail.HumanVerificationDetails) (protocol.Event, error) {
	if err := ctx.Err(); err != nil {
		return f.eventError("authentication cancelled")
	}

	session, err := f.client.Auth(f.username, f.loginPassword, f.info, hv)
	if err != nil {
		var hvErr *protonmail.HumanVerificationRequiredError
		if errors.As(err, &hvErr) {
			return f.requireCaptcha(hvErr.Details)
		}
		return f.mapError(err)
	}
	if session == nil {
		return f.fail("Proton returned no authentication session")
	}

	f.session = session
	return f.afterAuth()
}

func (f *Flow) requireCaptcha(details protonmail.HumanVerificationDetails) (protocol.Event, error) {
	if !supportsCaptcha(details.Methods) {
		return f.fail(unsupportedVerificationMessage(details.Methods))
	}

	details.Methods = append([]string(nil), details.Methods...)
	f.challenge = &details
	f.state = stateCaptcha

	return protocol.Event{
		Event:   protocol.EventCaptchaRequired,
		URL:     verificationURL(details),
		Methods: append([]string(nil), details.Methods...),
	}, nil
}

func (f *Flow) continueCaptcha(ctx context.Context, resp protocol.Response) (protocol.Event, error) {
	if !resp.VerificationDone && resp.CaptchaToken == "" {
		return f.eventError("human verification is not complete")
	}

	details := *f.challenge
	details.Methods = append([]string(nil), f.challenge.Methods...)
	if resp.CaptchaToken != "" {
		// A supplied token is a solved-CAPTCHA fallback; keep the original methods.
		details.Token = resp.CaptchaToken
	}

	return f.authenticate(ctx, &details)
}

func (f *Flow) continueTOTP(resp protocol.Response) (protocol.Event, error) {
	if strings.TrimSpace(resp.TOTP) == "" {
		return f.eventError("TOTP code is required")
	}

	scope, err := f.client.AuthTOTP(resp.TOTP)
	if err != nil {
		return f.mapError(err)
	}
	f.session.Scope = scope

	return f.afterSecondFactor()
}

func (f *Flow) continueMailbox(resp protocol.Response) (protocol.Event, error) {
	if resp.MailboxPassword == "" {
		return f.eventError("mailbox password is required")
	}

	return f.finish(resp.MailboxPassword)
}

func (f *Flow) afterAuth() (protocol.Event, error) {
	if f.session.TwoFactor.Enabled != 0 {
		if f.session.TwoFactor.TOTP != 1 {
			return f.fail("only TOTP two-factor authentication is supported")
		}
		f.state = stateTOTP
		return protocol.Event{Event: protocol.EventTOTPRequired}, nil
	}

	return f.afterSecondFactor()
}

func (f *Flow) afterSecondFactor() (protocol.Event, error) {
	if f.session.PasswordMode == protonmail.PasswordTwo {
		f.state = stateMailbox
		return protocol.Event{Event: protocol.EventMailboxPasswordRequired}, nil
	}

	// A single-password account uses the login password for the mailbox.
	return f.finish(f.loginPassword)
}

func (f *Flow) finish(mailboxPassword string) (protocol.Event, error) {
	salts, err := f.client.ListKeySalts()
	if err != nil {
		return f.mapError(err)
	}

	if _, err := f.client.Unlock(f.session, salts, mailboxPassword); err != nil {
		return f.mapError(err)
	}

	login, email, displayName, err := f.client.Identity()
	if err != nil {
		return f.mapError(err)
	}

	secretKey, bridgePassword, err := auth.GeneratePassword()
	if err != nil {
		return f.fail("failed to generate the bridge password: " + err.Error())
	}

	account := strings.TrimSpace(login)
	if account == "" {
		account = f.username
	}

	cached := &auth.CachedAuth{
		Auth:            *f.session,
		LoginPassword:   f.loginPassword,
		MailboxPassword: mailboxPassword,
		KeySalts:        salts,
	}
	if err := auth.EncryptAndSave(cached, account, secretKey); err != nil {
		return f.fail("failed to save the Proton credentials: " + err.Error())
	}

	f.state = stateDone
	return protocol.Event{
		Event:          protocol.EventSuccess,
		LoginUsername:  account,
		Email:          email,
		DisplayName:    displayName,
		BridgePassword: bridgePassword,
	}, nil
}

func (f *Flow) mapError(err error) (protocol.Event, error) {
	var apiErr *protonmail.APIError
	if errors.As(err, &apiErr) {
		if apiErr.Code == 2028 {
			// 2028 is terminal and must not be retried.
			f.state = stateDone
			message := AccessLimitedMessage
			if detail := strings.TrimSpace(apiErr.Message); detail != "" {
				message = fmt.Sprintf("%s (%s)", message, detail)
			}
			return protocol.Event{
				Event:         protocol.EventAccessLimited,
				Message:       message,
				AppealURL:     AppealURL,
				LoginUsername: f.username,
			}, nil
		}

		if apiErr.Message != "" {
			return f.fail(fmt.Sprintf("Proton API error %d: %s", apiErr.Code, apiErr.Message))
		}
		return f.fail(fmt.Sprintf("Proton API error %d", apiErr.Code))
	}

	return f.fail(err.Error())
}

// eventError reports a caller error without abandoning a pending step.
func (f *Flow) eventError(message string) (protocol.Event, error) {
	message = strings.TrimSpace(message)
	if message == "" {
		message = "authentication failed"
	}
	return protocol.Event{Event: protocol.EventError, Message: message}, errors.New(message)
}

// fail reports a terminal error and marks the flow finished.
func (f *Flow) fail(message string) (protocol.Event, error) {
	f.state = stateDone
	return f.eventError(message)
}

func supportsCaptcha(methods []string) bool {
	for _, method := range methods {
		if strings.EqualFold(method, captchaMethod) {
			return true
		}
	}
	return false
}

func unsupportedVerificationMessage(methods []string) string {
	if len(methods) == 0 {
		return "unsupported human verification method: only CAPTCHA is supported"
	}
	return fmt.Sprintf("unsupported human verification methods %q: only CAPTCHA is supported", strings.Join(methods, ","))
}

func verificationURL(details protonmail.HumanVerificationDetails) string {
	q := url.Values{}
	q.Set("methods", strings.Join(details.Methods, ","))
	q.Set("token", details.Token)
	return verifyBaseURL + "?" + q.Encode()
}
