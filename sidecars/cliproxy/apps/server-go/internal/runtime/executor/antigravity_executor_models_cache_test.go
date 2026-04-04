package executor

import (
	"testing"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/registry"
)

func resetAntigravityPrimaryModelsCacheForTest() {
	antigravityPrimaryModelsCache.mu.Lock()
	antigravityPrimaryModelsCache.models = nil
	antigravityPrimaryModelsCache.mu.Unlock()
}

func TestStoreAntigravityPrimaryModels_EmptyDoesNotOverwrite(t *testing.T) {
	resetAntigravityPrimaryModelsCacheForTest()
	t.Cleanup(resetAntigravityPrimaryModelsCacheForTest)

	seed := []*registry.ModelInfo{
		{ID: "claude-sonnet-4-5"},
		{ID: "gpt-5"},
	}
	if updated := storeAntigravityPrimaryModels(seed); !updated {
		t.Fatal("expected non-empty model list to update primary cache")
	}

	if updated := storeAntigravityPrimaryModels(nil); updated {
		t.Fatal("expected nil model list not to overwrite primary cache")
	}
	if updated := storeAntigravityPrimaryModels([]*registry.ModelInfo{}); updated {
		t.Fatal("expected empty model list not to overwrite primary cache")
	}

	got := loadAntigravityPrimaryModels()
	if len(got) != 2 {
		t.Fatalf("expected cached model count 2, got %d", len(got))
	}
	if got[0].ID != "claude-sonnet-4-5" || got[1].ID != "gpt-5" {
		t.Fatalf("unexpected cached model ids: %q, %q", got[0].ID, got[1].ID)
	}
}

func TestLoadAntigravityPrimaryModels_ReturnsClone(t *testing.T) {
	resetAntigravityPrimaryModelsCacheForTest()
	t.Cleanup(resetAntigravityPrimaryModelsCacheForTest)

	if updated := storeAntigravityPrimaryModels([]*registry.ModelInfo{{
		ID:                         "gpt-5",
		DisplayName:                "GPT-5",
		SupportedGenerationMethods: []string{"generateContent"},
		SupportedParameters:        []string{"temperature"},
		Thinking: &registry.ThinkingSupport{
			Levels: []string{"high"},
		},
	}}); !updated {
		t.Fatal("expected model cache update")
	}

	got := loadAntigravityPrimaryModels()
	if len(got) != 1 {
		t.Fatalf("expected one cached model, got %d", len(got))
	}
	got[0].ID = "mutated-id"
	if len(got[0].SupportedGenerationMethods) > 0 {
		got[0].SupportedGenerationMethods[0] = "mutated-method"
	}
	if len(got[0].SupportedParameters) > 0 {
		got[0].SupportedParameters[0] = "mutated-parameter"
	}
	if got[0].Thinking != nil && len(got[0].Thinking.Levels) > 0 {
		got[0].Thinking.Levels[0] = "mutated-level"
	}

	again := loadAntigravityPrimaryModels()
	if len(again) != 1 {
		t.Fatalf("expected one cached model after mutation, got %d", len(again))
	}
	if again[0].ID != "gpt-5" {
		t.Fatalf("expected cached model id to remain %q, got %q", "gpt-5", again[0].ID)
	}
	if len(again[0].SupportedGenerationMethods) == 0 || again[0].SupportedGenerationMethods[0] != "generateContent" {
		t.Fatalf("expected cached generation methods to be unmutated, got %v", again[0].SupportedGenerationMethods)
	}
	if len(again[0].SupportedParameters) == 0 || again[0].SupportedParameters[0] != "temperature" {
		t.Fatalf("expected cached supported parameters to be unmutated, got %v", again[0].SupportedParameters)
	}
	if again[0].Thinking == nil || len(again[0].Thinking.Levels) == 0 || again[0].Thinking.Levels[0] != "high" {
		t.Fatalf("expected cached model thinking levels to be unmutated, got %v", again[0].Thinking)
	}
}

func TestEnsureAntigravityPublicImageModels_AddsMissingFlashImagePreviewModel(t *testing.T) {
	input := []*registry.ModelInfo{
		{ID: "gemini-2.5-flash", DisplayName: "Gemini 2.5 Flash"},
		{ID: "gemini-3.1-pro-preview", DisplayName: "Gemini 3.1 Pro Preview"},
	}

	got := ensureAntigravityPublicImageModels(input)

	want := map[string]struct{}{
		"gemini-2.5-flash":               {},
		"gemini-3.1-pro-preview":         {},
		"gemini-3.1-flash-image-preview": {},
	}
	if len(got) != len(want) {
		t.Fatalf("model count = %d, want %d", len(got), len(want))
	}

	seen := map[string]*registry.ModelInfo{}
	for _, model := range got {
		if model == nil {
			continue
		}
		seen[model.ID] = model
	}
	for id := range want {
		if _, ok := seen[id]; !ok {
			t.Fatalf("expected model %q in augmented antigravity primary models", id)
		}
	}

	model := seen["gemini-3.1-flash-image-preview"]
	if model == nil {
		t.Fatal("missing model gemini-3.1-flash-image-preview")
	}
	if len(model.SupportedGenerationMethods) == 0 || model.SupportedGenerationMethods[0] != "generateContent" {
		t.Fatalf("expected generateContent support for %q, got %v", model.ID, model.SupportedGenerationMethods)
	}
}
