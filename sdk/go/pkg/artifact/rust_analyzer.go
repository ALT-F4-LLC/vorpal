package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustAnalyzer(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "4bd3745cd87cc821da649df3f115cf499344f8dda3c6c7fd9b291a17752d0d88"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "4f73ce246976fd7e39d91a91d1b0ffd58c363f3de50c20df081e4174b85a20c7"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "f685e474f491f87616f6643b2c1c5dd9f3fde1eda1a9c16e9c2b3533fe0f52b4"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "e56c917b1b3f0b8334df05ea8cbbbeeae3371052a249c1a5a606662469a9a48f"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
