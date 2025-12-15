package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Staticcheck(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("staticcheck:2025.1.1")
}
