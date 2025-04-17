package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func ProtocGenGoGRPC(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "2b95070977138f3b351171001e152ef147fcbdd31ec191e29725dcfc7ad88322"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "c6c2063cff14575636115118cbbfe097eaf5d186a7711f4c15c343f867b77ac6"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "baa55e4e82fab6b9fb918d9b79717fdbcdf6447ee469ab20637088454c8f88ae"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
