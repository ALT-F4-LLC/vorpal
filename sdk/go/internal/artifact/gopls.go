package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Gopls(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "6d597686fa68271ef9367eae65aa6cf997e48def78a941a93aea4c96183b457a"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "2bdbb5ae2632da58df6087b959cf7e6867c2ad534297428de22877de32aea5ca"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "761fcbb77a80c668982e378e4e9bc5c03183c4e7cc70aa13660176422093aa3c"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
