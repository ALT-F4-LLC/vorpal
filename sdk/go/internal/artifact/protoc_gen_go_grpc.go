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
		digest = "22f504b558607cb98545fd4a119d9aa2c8afdf1d5abe930fca8f7f67a638326b"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "3c3a7674049be0e8babb4feac647af64bc070a2d0fdfe1f329ee4f99710676d0"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "54b67d3c7eab2d300826028ef649eb14e1f2f1a80e2a895338b453c30e311973"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
