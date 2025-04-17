package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Staticcheck(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "b33f2409ae3c92fe09cb9fdc00b0b1ccba95f673b493f08d7f0c44edf9b3ae06"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "c1a3711365894fe56cb3f4bd2d14dc92961c2b8bcd2c9336e2708162a05176a3"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "a891626feafc92ae75ace871a4601cb157435bca7e42d83cc786a01f1982cc2b"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "4f1582d57a4acd676af98847bfea6e1716b629691b614027ec9d59841aa0ffe8"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
