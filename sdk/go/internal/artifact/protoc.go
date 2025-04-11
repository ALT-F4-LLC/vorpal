package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Protoc(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("8ad451bdcda8f24f4af59ccca23fd71a06975a9d069571f19b9a0d503f8a65c8")
}
