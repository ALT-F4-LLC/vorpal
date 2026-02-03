package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type ociImageScriptData struct {
	ArtifactsList string
	Crane         string
	Name          string
	Namespace     string
	Rootfs        string
	Rsync         string
}

type OciImage struct {
	aliases   []string
	artifacts []*string
	crane     *string
	name      string
	rootfs    string
	rsync     *string
}

const ociImageScript = `
OCI_IMAGE_ARTIFACTS="{{.ArtifactsList}}"
OCI_IMAGE_CRANE="{{.Crane}}"
OCI_IMAGE_NAME="{{.Name}}"
OCI_IMAGE_ROOTFS="{{.Rootfs}}"
OCI_IMAGE_RSYNC="{{.Rsync}}"
OUTPUT_TAR=${PWD}/rootfs.tar
ROOTFS_DIR=${PWD}/rootfs
STORE_PREFIX=var/lib/vorpal/store/artifact/output/{{.Namespace}}

# Detect platform based on build architecture
case "$(uname -m)" in
    x86_64)  OCI_PLATFORM="linux/amd64" ;;
    aarch64) OCI_PLATFORM="linux/arm64" ;;
    *)       OCI_PLATFORM="linux/$(uname -m)" ;;
esac

mkdir -pv ${ROOTFS_DIR}

for artifact in ${OCI_IMAGE_ARTIFACTS}; do
    SOURCE_DIR=/${STORE_PREFIX}/${artifact}
    TARGET_PATH=${STORE_PREFIX}/${artifact}

    mkdir -p ${ROOTFS_DIR}/${TARGET_PATH}

    echo "Copying artifact layer ${artifact}..."

    ${OCI_IMAGE_RSYNC}/bin/rsync -aW ${SOURCE_DIR}/ ${ROOTFS_DIR}/${TARGET_PATH}

    echo "Copied artifact layer ${artifact}"

    # Symlink bin files to /usr/local/bin
    if [ -d "${SOURCE_DIR}/bin" ]; then
        mkdir -p ${ROOTFS_DIR}/usr/local/bin
        for bin_file in ${SOURCE_DIR}/bin/*; do
            if [ -f "${bin_file}" ]; then
                bin_name=$(basename "${bin_file}")
                ln -sf /${TARGET_PATH}/bin/${bin_name} ${ROOTFS_DIR}/usr/local/bin/${bin_name}
                echo "Symlinked ${bin_name} to /usr/local/bin"
            fi
        done
    fi
done

echo "Copying Vorpal operating system files..."

${OCI_IMAGE_RSYNC}/bin/rsync -aW ${OCI_IMAGE_ROOTFS}/ ${ROOTFS_DIR}

echo "Copied Vorpal operating system files"

echo "Creating output tarball..."

tar -cf ${OUTPUT_TAR} -C ${ROOTFS_DIR} .

echo "Created output tarball"

mkdir -p ${VORPAL_OUTPUT}

echo "Creating OCI image ${OCI_IMAGE_NAME}:latest"

${OCI_IMAGE_CRANE}/bin/crane append \
    --new_layer ${OUTPUT_TAR} \
    --new_tag ${OCI_IMAGE_NAME}:latest \
    --oci-empty-base \
    --output ${VORPAL_OUTPUT}/image.tar \
    --platform ${OCI_PLATFORM}

echo "Setting platform metadata in image config..."

${OCI_IMAGE_CRANE}/bin/crane mutate \
    --set-platform ${OCI_PLATFORM} \
    --output ${VORPAL_OUTPUT}/image-mutated.tar \
    ${VORPAL_OUTPUT}/image.tar

mv ${VORPAL_OUTPUT}/image-mutated.tar ${VORPAL_OUTPUT}/image.tar

echo "Created OCI image ${OCI_IMAGE_NAME}:latest"`

func NewOciImage(name string, rootfs string) *OciImage {
	return &OciImage{
		artifacts: []*string{},
		crane:     nil,
		name:      name,
		rootfs:    rootfs,
		rsync:     nil,
	}
}

func (o *OciImage) WithArtifacts(artifacts []*string) *OciImage {
	o.artifacts = artifacts
	return o
}

func (o *OciImage) WithCrane(crane *string) *OciImage {
	o.crane = crane
	return o
}

func (o *OciImage) WithRootfs(rootfs string) *OciImage {
	o.rootfs = rootfs
	return o
}

func (o *OciImage) WithAliases(aliases []string) *OciImage {
	o.aliases = aliases
	return o
}

func (o *OciImage) Build(context *config.ConfigContext) (*string, error) {
	// Validate image name
	if o.name != strings.ToLower(o.name) {
		return nil, fmt.Errorf("container image name must be lowercase: '%s'", o.name)
	}
	for _, c := range o.name {
		if !((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') ||
			c == '/' || c == ':' || c == '-' || c == '.' || c == '_') {
			return nil, fmt.Errorf(
				"container image name contains invalid character '%c': '%s'. "+
					"Allowed: lowercase letters, digits, and / : - . _",
				c, o.name)
		}
	}

	var crane *string
	if o.crane != nil {
		crane = o.crane
	} else {
		c, err := Crane(context)
		if err != nil {
			return nil, err
		}
		crane = c
	}

	var rsync *string
	if o.rsync != nil {
		rsync = o.rsync
	} else {
		r, err := Rsync(context)
		if err != nil {
			return nil, err
		}
		rsync = r
	}

	artifactDigests := make([]string, len(o.artifacts))
	for i, artifact := range o.artifacts {
		artifactDigests[i] = *artifact
	}

	scriptTemplate, err := template.New("script").Parse(ociImageScript)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := ociImageScriptData{
		ArtifactsList: strings.Join(artifactDigests, " "),
		Crane:         GetEnvKey(*crane),
		Name:          o.name,
		Namespace:     context.GetArtifactNamespace(),
		Rootfs:        GetEnvKey(o.rootfs),
		Rsync:         GetEnvKey(*rsync),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	stepArtifacts := []*string{crane, rsync, &o.rootfs}
	stepArtifacts = append(stepArtifacts, o.artifacts...)

	step, err := Shell(context, stepArtifacts, nil, scriptBuffer.String(), nil)
	if err != nil {
		return nil, err
	}

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(o.name, []*api.ArtifactStep{step}, systems).
		WithAliases(o.aliases).
		Build(context)
}
