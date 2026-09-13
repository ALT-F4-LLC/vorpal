package artifact

import (
	"errors"
	"fmt"
	"strings"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustToolchainTarget(system string) (*string, error) {
	var target string

	switch system {
	case "aarch64-darwin":
		target = "aarch64-apple-darwin"
	case "aarch64-linux":
		target = "aarch64-unknown-linux-gnu"
	case "x86_64-darwin":
		target = "x86_64-apple-darwin"
	case "x86_64-linux":
		target = "x86_64-unknown-linux-gnu"
	default:
		return nil, errors.New("unsupported 'rust-toolchain' system")
	}

	return &target, nil
}

func RustToolchainVersion() string {
	return "1.93.1"
}

func RustToolchain(context *config.ConfigContext) (*string, error) {
	cargo, err := Cargo(context)
	if err != nil {
		return nil, err
	}

	clippy, err := Clippy(context)
	if err != nil {
		return nil, err
	}

	rustAnalyzer, err := RustAnalyzer(context)
	if err != nil {
		return nil, err
	}

	rustSrc, err := RustSrc(context)
	if err != nil {
		return nil, err
	}

	rustStd, err := RustStd(context)
	if err != nil {
		return nil, err
	}

	rustc, err := Rustc(context)
	if err != nil {
		return nil, err
	}

	rustfmt, err := Rustfmt(context)
	if err != nil {
		return nil, err
	}

	artifacts := []*string{cargo, clippy, rustAnalyzer, rustSrc, rustStd, rustc, rustfmt}

	componentPaths := make([]string, len(artifacts))
	for i, a := range artifacts {
		componentPaths[i] = GetEnvKey(*a)
	}

	toolchainTarget, err := RustToolchainTarget(context.GetTargetStr())
	if err != nil {
		return nil, err
	}

	toolchainVersion := RustToolchainVersion()

	stepScript := fmt.Sprintf(`toolchain_dir="$VORPAL_OUTPUT/toolchains/%s-%s"

mkdir -p "$toolchain_dir"

components=(%s)

echo "Copying Rust toolchain components to $toolchain_dir..."

for component in "${components[@]}"; do
    echo "Processing component: $component"

    find "$component" | while read -r file; do
        relative_path=$(echo "$file" | sed -e "s|$component||")

        if [[ "$relative_path" == "/manifest.in" ]]; then
            continue
        fi

        if [ -d "$file" ]; then
            mkdir -p "$toolchain_dir$relative_path"
        else
            cp -p "$file" "$toolchain_dir$relative_path"
        fi
    done
done

cat > "$VORPAL_OUTPUT/settings.toml" << "EOF"
auto_self_update = "disable"
profile = "minimal"
version = "12"

[overrides]
EOF`, toolchainVersion, *toolchainTarget, strings.Join(componentPaths, " "))

	step, err := Shell(context, artifacts, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	name := "rust-toolchain"

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, toolchainVersion)}).
		Build(context)
}
