package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Goimports(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "29982a2905a452335d880f2873378033621eda6c909fb8f5beec1d0963b4054d"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "5566c109c9025bad171bdd071b31be7085d8b1fd9c69e43a22eccfdfb4f6b885"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "e3b193219819ee0bdb49e3744a238a2ac182d2546ed500823072b00960b45dcc"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "9eb303ffce16aff362edb0b722976d770d14d0ce39c490865b60811408239584"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
