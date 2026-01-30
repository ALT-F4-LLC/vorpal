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
	Rootfs        string
	Rsync         string
}

const ociImageScript = `
OCI_IMAGE_ARTIFACTS="{{.ArtifactsList}}"
OCI_IMAGE_CRANE="{{.Crane}}"
OCI_IMAGE_NAME="{{.Name}}"
OCI_IMAGE_ROOTFS="{{.Rootfs}}"
OCI_IMAGE_RSYNC="{{.Rsync}}"
OUTPUT_TAR=${PWD}/rootfs.tar
ROOTFS_DIR=${PWD}/rootfs
STORE_PREFIX=var/lib/vorpal/store/artifact/output/library

mkdir -pv ${ROOTFS_DIR}

for artifact in ${OCI_IMAGE_ARTIFACTS}; do
    SOURCE_DIR=/${STORE_PREFIX}/${artifact}
    TARGET_PATH=${STORE_PREFIX}/${artifact}

    mkdir -p ${ROOTFS_DIR}/${TARGET_PATH}

    echo "Copying artifact layer ${artifact}..."

    ${OCI_IMAGE_RSYNC}/bin/rsync -aPW ${SOURCE_DIR}/ ${ROOTFS_DIR}/${TARGET_PATH}

    echo "Copied artifact layer ${artifact}"
done

echo "Copying Vorpal operating system files..."

${OCI_IMAGE_RSYNC}/bin/rsync -aPW ${OCI_IMAGE_ROOTFS}/ ${ROOTFS_DIR}

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
    --output ${VORPAL_OUTPUT}/image.tar
`

type OciImage struct {
	aliases   []string
	artifacts []*string
	crane     *string
	rootfs    *string
	name      string
}

func NewOciImage(name string) *OciImage {
	return &OciImage{
		artifacts: []*string{},
		crane:     nil,
		rootfs:    nil,
		name:      name,
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

func (o *OciImage) WithRootfs(rootfs *string) *OciImage {
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

	var rootfs *string
	if o.rootfs != nil {
		rootfs = o.rootfs
	} else {
		r, err := context.FetchArtifactAlias("library/linux-vorpal-slim:latest")
		if err != nil {
			return nil, err
		}
		rootfs = r
	}

	rsync, err := Rsync(context)
	if err != nil {
		return nil, err
	}

	artifactName := fmt.Sprintf("oci-image-%s",
		strings.ReplaceAll(strings.ReplaceAll(strings.ReplaceAll(o.name, ":", "-"), "/", "-"), ".", "-"))

	artifactDigests := make([]string, len(o.artifacts))
	for i, artifact := range o.artifacts {
		artifactDigests[i] = *artifact
	}

	scriptTemplate, err := template.New("script").Parse(BashScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := ociImageScriptData{
		ArtifactsList: strings.Join(artifactDigests, " "),
		Crane:         *crane,
		Name:          o.name,
		Rootfs:        *rootfs,
		Rsync:         *rsync,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	stepArtifacts := []*string{crane, rsync, rootfs}
	stepArtifacts = append(stepArtifacts, o.artifacts...)

	step, err := Shell(context, stepArtifacts, nil, scriptBuffer.String(), nil)
	if err != nil {
		return nil, err
	}

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(artifactName, []*api.ArtifactStep{step}, systems).
		WithAliases(o.aliases).
		Build(context)
}
