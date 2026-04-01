package linux_vorpal

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
)

func init() {
	artifact.LinuxVorpalBuilder = Build
}
