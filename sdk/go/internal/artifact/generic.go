package artifact

import (
	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/context"
)

func AddArtifact(
	ctx *context.ConfigContext,
	artifacts []*artifactApi.ArtifactId,
	environments map[string]string,
	name string,
	script string,
	sources []*artifactApi.ArtifactSourceId,
	systems []string,
) *artifactApi.ArtifactId {
	// TODO: implement AddArtifact function

	return nil
}
