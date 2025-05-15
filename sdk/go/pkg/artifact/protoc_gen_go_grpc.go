package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGoGRPC(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "410549ec26b1b169c64ca0d4b6c09987000b0e88b7854a608708806c58a13dcc"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "ab16f99021ec5cf3b896b90c4c26bdc3a674d34c0f1d6afa06c490b20e480e47"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "ee3fee174a60350ba21e971557b2ae189fc7674127d6e528fd78aa8d151d98c8"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "ed50829e4f561a392881bb81b10c6617e7ecf08a0dc8a06e9515c559f51f3ffa"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
