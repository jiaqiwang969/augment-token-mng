package oauthcreds

import "testing"

func TestResolveGoogleCredentialsPrefersMetadata(t *testing.T) {
	t.Setenv(GeminiClientIDEnv, "env-client-id")
	t.Setenv(GeminiClientSecretEnv, "env-client-secret")

	clientID, clientSecret := ResolveGoogleCredentials(
		map[string]any{
			"client_id":     "meta-client-id",
			"client_secret": "meta-client-secret",
		},
		GeminiClientIDEnv,
		GeminiClientSecretEnv,
	)

	if clientID != "meta-client-id" {
		t.Fatalf("expected metadata client_id, got %q", clientID)
	}
	if clientSecret != "meta-client-secret" {
		t.Fatalf("expected metadata client_secret, got %q", clientSecret)
	}
}

func TestResolveGoogleCredentialsFallsBackToEnvironment(t *testing.T) {
	t.Setenv(AntigravityClientIDEnv, "env-client-id")
	t.Setenv(AntigravityClientSecretEnv, "env-client-secret")

	clientID, clientSecret := ResolveGoogleCredentials(nil, AntigravityClientIDEnv, AntigravityClientSecretEnv)

	if clientID != "env-client-id" {
		t.Fatalf("expected env client_id, got %q", clientID)
	}
	if clientSecret != "env-client-secret" {
		t.Fatalf("expected env client_secret, got %q", clientSecret)
	}
}
