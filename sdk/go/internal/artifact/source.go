package artifact

type ArtifactSource struct {
	Excludes []string
	Hash     *string
	Includes []string
	Path     string
}

type ArtifactSourceKind string

const (
	ArtifactSourceKind_GIT     ArtifactSourceKind = "GIT"
	ArtifactSourceKind_HTTP    ArtifactSourceKind = "HTTP"
	ArtifactSourceKind_LOCAL   ArtifactSourceKind = "LOCAL"
	ArtifactSourceKind_UNKNOWN ArtifactSourceKind = "UNKNOWN"
)
