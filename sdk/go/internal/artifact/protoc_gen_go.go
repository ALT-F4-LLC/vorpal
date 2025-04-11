package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("47a94d59d206be31eef2214418fce60570e7a9a175f96eeab02d1c9c3c7d0ed9")
}
