package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Pnpm(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("pnpm:10.5.2")
}
