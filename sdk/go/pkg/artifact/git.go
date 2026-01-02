package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Git(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("git:2.52.0")
}
