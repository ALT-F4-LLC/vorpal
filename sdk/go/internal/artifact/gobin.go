package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func GoBin(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("6f024d78f0957297229cb00b74b9544fb2c4708a465a584b1e02dfbe5f71922b")
}
