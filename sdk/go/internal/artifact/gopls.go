package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Gopls(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("91cc5dc188bc8bc5e6ff63aede51540ae15c7980ab898e1b0afb6db79657fa43")
}
