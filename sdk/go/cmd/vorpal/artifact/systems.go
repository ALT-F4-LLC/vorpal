package artifact

import (
	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

var SYSTEMS = []api.ArtifactSystem{
	api.ArtifactSystem_AARCH64_DARWIN,
	api.ArtifactSystem_AARCH64_LINUX,
	api.ArtifactSystem_X8664_DARWIN,
	api.ArtifactSystem_X8664_LINUX,
}
