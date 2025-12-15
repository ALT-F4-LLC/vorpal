package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGoGRPC(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("protoc-gen-go-grpc:1.70.0")
}
