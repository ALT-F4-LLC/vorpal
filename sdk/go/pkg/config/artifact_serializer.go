package config

// Custom artifact JSON serializers that produce output matching Rust's
// serde_json::to_vec for prost-generated structs. The proto-generated Go
// structs use `json:",omitempty"` tags which omit empty slices and zero
// values; Rust's serde includes ALL fields. Cross-SDK digest parity
// requires byte-identical JSON.
//
// Rules (matching Rust serde_json + prost Serialize derive):
//   - Field names are snake_case (matching proto field names)
//   - Field order follows proto field number order
//   - ALL fields are always present (even zero values, empty arrays)
//   - Enums serialize as integers (not strings)
//   - Optional None serializes as null
//   - Empty repeated fields serialize as []
//   - Empty strings serialize as ""

import (
	"encoding/json"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

// serializableArtifactStepSecret mirrors ArtifactStepSecret with fields in
// proto field number order and no omitempty tags.
type serializableArtifactStepSecret struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

// serializableArtifactSource mirrors ArtifactSource with fields in proto
// field number order and no omitempty tags. Digest is *string so that nil
// serializes as JSON null (matching Rust's Option<String>::None).
type serializableArtifactSource struct {
	Digest   *string  `json:"digest"`
	Excludes []string `json:"excludes"`
	Includes []string `json:"includes"`
	Name     string   `json:"name"`
	Path     string   `json:"path"`
}

// serializableArtifactStep mirrors ArtifactStep with fields in proto field
// number order and no omitempty tags. Entrypoint and Script are *string so
// that nil serializes as JSON null.
type serializableArtifactStep struct {
	Entrypoint   *string                          `json:"entrypoint"`
	Script       *string                          `json:"script"`
	Secrets      []serializableArtifactStepSecret `json:"secrets"`
	Arguments    []string                         `json:"arguments"`
	Artifacts    []string                         `json:"artifacts"`
	Environments []string                         `json:"environments"`
}

// serializableArtifact mirrors Artifact with fields in proto field number
// order and no omitempty tags. Target and Systems use int32 to serialize
// enums as integers.
type serializableArtifact struct {
	Target  int32                        `json:"target"`
	Sources []serializableArtifactSource `json:"sources"`
	Steps   []serializableArtifactStep   `json:"steps"`
	Systems []int32                      `json:"systems"`
	Aliases []string                     `json:"aliases"`
	Name    string                       `json:"name"`
}

// ensureStringSlice returns the input slice if non-nil, or an initialized
// empty slice. This ensures json.Marshal produces [] instead of null.
func ensureStringSlice(s []string) []string {
	if s == nil {
		return make([]string, 0)
	}
	return s
}

// serializeArtifactStepSecret converts a proto ArtifactStepSecret to the
// serializable form matching Rust's serde output.
func serializeArtifactStepSecret(secret *artifact.ArtifactStepSecret) serializableArtifactStepSecret {
	return serializableArtifactStepSecret{
		Name:  secret.GetName(),
		Value: secret.GetValue(),
	}
}

// serializeArtifactSource converts a proto ArtifactSource to the
// serializable form matching Rust's serde output.
func serializeArtifactSource(source *artifact.ArtifactSource) serializableArtifactSource {
	return serializableArtifactSource{
		Digest:   source.Digest, // nil -> JSON null, non-nil -> string
		Excludes: ensureStringSlice(source.GetExcludes()),
		Includes: ensureStringSlice(source.GetIncludes()),
		Name:     source.GetName(),
		Path:     source.GetPath(),
	}
}

// serializeArtifactStep converts a proto ArtifactStep to the serializable
// form matching Rust's serde output.
func serializeArtifactStep(step *artifact.ArtifactStep) serializableArtifactStep {
	secrets := make([]serializableArtifactStepSecret, 0, len(step.GetSecrets()))
	for _, s := range step.GetSecrets() {
		secrets = append(secrets, serializeArtifactStepSecret(s))
	}

	return serializableArtifactStep{
		Entrypoint:   step.Entrypoint, // nil -> JSON null
		Script:       step.Script,     // nil -> JSON null
		Secrets:      secrets,
		Arguments:    ensureStringSlice(step.GetArguments()),
		Artifacts:    ensureStringSlice(step.GetArtifacts()),
		Environments: ensureStringSlice(step.GetEnvironments()),
	}
}

// SerializeArtifactJSON produces a JSON byte slice for the given Artifact
// that is byte-identical to Rust's serde_json::to_vec output. This is the
// critical path for cross-SDK digest parity.
func SerializeArtifactJSON(a *artifact.Artifact) ([]byte, error) {
	sources := make([]serializableArtifactSource, 0, len(a.GetSources()))
	for _, s := range a.GetSources() {
		sources = append(sources, serializeArtifactSource(s))
	}

	steps := make([]serializableArtifactStep, 0, len(a.GetSteps()))
	for _, s := range a.GetSteps() {
		steps = append(steps, serializeArtifactStep(s))
	}

	systems := make([]int32, 0, len(a.GetSystems()))
	for _, s := range a.GetSystems() {
		systems = append(systems, int32(s))
	}

	sa := serializableArtifact{
		Target:  int32(a.GetTarget()),
		Sources: sources,
		Steps:   steps,
		Systems: systems,
		Aliases: ensureStringSlice(a.GetAliases()),
		Name:    a.GetName(),
	}

	return json.Marshal(sa)
}
