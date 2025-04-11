package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Goimports(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("112f8c42be33bfa5274fcdff2748cd68eae755adbae1cbd70cc012531375d7c1")
}
