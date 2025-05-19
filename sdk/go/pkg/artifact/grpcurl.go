package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Grpcurl(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("grpcurl:1.9.3")
}
