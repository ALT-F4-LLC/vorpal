package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Grpcurl(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "aaa487c3dc4092aac62818b332e9569a57f89af773aaa574015e766a692e5670"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "3c05329da72f300dc01be7fef11ce7e9e4beb1c300098c9cedcfdecc99d4318b"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "8d0257c581b2021a49a97cbbc2f77891bda0eb0f7d0c4dffd9fe166fa00b1db7"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "7753f7c7187a34e769d9b2d649a58b13a093a00f76c71a632ec797d4e779d0c4"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
