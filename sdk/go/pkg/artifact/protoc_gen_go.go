package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("protoc-gen-go:1.36.3")
}
