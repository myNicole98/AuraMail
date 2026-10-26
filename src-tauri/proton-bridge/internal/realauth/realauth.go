// Package realauth adapts Hydroxide's Proton API client to authflow.ProtonAuth.
package realauth

import (
	"errors"

	"codeberg.org/emersion/hydroxide/internal/authflow"
	"codeberg.org/emersion/hydroxide/protonmail"
)

const (
	// DefaultAPIEndpoint is Proton's production API root.
	DefaultAPIEndpoint = "https://mail.proton.me/api"
	// DefaultAppVersion is Hydroxide's upstream fallback client identifier.
	// Proton's acceptance of it still needs live verification.
	DefaultAppVersion = "Other"
)

var _ authflow.ProtonAuth = (*Client)(nil)

// Client is a Hydroxide Proton API client that remembers the login username
// needed by Identity.
type Client struct {
	*protonmail.Client
	username string
}

// New returns a client for Proton's production API.
func New() *Client {
	return NewForEndpoint(DefaultAPIEndpoint)
}

// NewForEndpoint returns a client for rootURL. An empty rootURL uses the
// production endpoint.
func NewForEndpoint(rootURL string) *Client {
	if rootURL == "" {
		rootURL = DefaultAPIEndpoint
	}
	return &Client{Client: &protonmail.Client{
		RootURL:    rootURL,
		AppVersion: DefaultAppVersion,
	}}
}

// AuthInfo records the login username before asking Proton for SRP parameters.
func (c *Client) AuthInfo(username string) (*protonmail.AuthInfo, error) {
	c.username = username
	return c.Client.AuthInfo(username)
}

// Auth records the login username and performs Proton's SRP exchange.
func (c *Client) Auth(username, password string, info *protonmail.AuthInfo, hv *protonmail.HumanVerificationDetails) (*protonmail.Auth, error) {
	c.username = username
	return c.Client.Auth(username, password, info, hv)
}

// Identity returns the login username and the address with the lowest Order.
func (c *Client) Identity() (login, primaryEmail, displayName string, err error) {
	addrs, err := c.ListAddresses()
	if err != nil {
		return "", "", "", err
	}

	var primary *protonmail.Address
	for _, addr := range addrs {
		if addr == nil || addr.Email == "" {
			continue
		}
		if primary == nil || addr.Order < primary.Order {
			primary = addr
		}
	}
	if primary == nil {
		return "", "", "", errors.New("realauth: Proton account has no address")
	}

	return c.username, primary.Email, primary.DisplayName, nil
}
