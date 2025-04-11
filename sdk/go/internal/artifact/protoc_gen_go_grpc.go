package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func ProtocGenGoGRPC(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("d4165e4b2ca1b82ddaed218d948ce10eca9a714405575ffccb0674eb069e3361")
}
