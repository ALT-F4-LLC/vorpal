package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Protoc(ctx *config.ConfigContext) (*string, error) {
	return ctx.FetchArtifact("protoc:25.4")
}
