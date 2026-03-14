package oauthcreds

import (
	"os"
	"strings"
)

const (
	GeminiClientIDEnv          = "CLIPROXY_GEMINI_OAUTH_CLIENT_ID"
	GeminiClientSecretEnv      = "CLIPROXY_GEMINI_OAUTH_CLIENT_SECRET"
	AntigravityClientIDEnv     = "CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID"
	AntigravityClientSecretEnv = "CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET"
)

func ResolveGoogleCredentials(metadata map[string]any, clientIDEnv, clientSecretEnv string) (string, string) {
	clientID := valueFromMetadata(metadata, "client_id")
	if clientID == "" {
		clientID = strings.TrimSpace(os.Getenv(clientIDEnv))
	}

	clientSecret := valueFromMetadata(metadata, "client_secret")
	if clientSecret == "" {
		clientSecret = strings.TrimSpace(os.Getenv(clientSecretEnv))
	}

	return clientID, clientSecret
}

func GeminiCredentials(metadata map[string]any) (string, string) {
	return ResolveGoogleCredentials(metadata, GeminiClientIDEnv, GeminiClientSecretEnv)
}

func AntigravityCredentials(metadata map[string]any) (string, string) {
	return ResolveGoogleCredentials(metadata, AntigravityClientIDEnv, AntigravityClientSecretEnv)
}

func valueFromMetadata(metadata map[string]any, key string) string {
	if len(metadata) == 0 {
		return ""
	}

	value, ok := metadata[key]
	if !ok {
		return ""
	}

	switch typed := value.(type) {
	case string:
		return strings.TrimSpace(typed)
	default:
		return ""
	}
}
