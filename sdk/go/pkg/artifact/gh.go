package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gh(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "a038ac34aeef1ac2acabafe4a99312e88bc4a611746d23d14c179b81123aa25e"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "57bb095cfcbfabbb06129e01463fd7162e7f60b09f0b39d86295c9eca35a75c6"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "ea0b5de3b00c3b223ac156924b4f2d5755c365d619d5b4d82f03dc5f96c90a99"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "0dad67637b40f105c3320f1667676c0615e3a7940fe29a44c6259bcc84bb78da"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
