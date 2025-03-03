package context

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	configApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/config"
	registryApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/registry"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/cli"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/store"
	"github.com/h2non/filetype"
	"github.com/mholt/archives"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

type ConfigContext struct {
	ArtifactId       map[*artifactApi.ArtifactId]*artifactApi.Artifact
	artifactSourceId map[string]*artifactApi.ArtifactSourceId
	port             int
	registry         string
	system           artifactApi.ArtifactSystem
}

type ConfigServer struct {
	configApi.UnimplementedConfigServiceServer
	Config  *configApi.Config
	Context *ConfigContext
}

func GetContext() *ConfigContext {
	startCmd, err := cli.NewStartCommand()
	if err != nil {
		log.Fatal(err)
	}

	return &ConfigContext{
		system: startCmd.Target,
	}
}

func NewConfigServer(context *ConfigContext, config *configApi.Config) *ConfigServer {
	return &ConfigServer{
		Config:  config,
		Context: context,
	}
}

func handleFile(outputPath string) archives.FileHandler {
	return func(ctx context.Context, info archives.FileInfo) error {
		outputFilePath := filepath.Join(outputPath, filepath.Clean(info.NameInArchive))

		if err := os.MkdirAll(filepath.Dir(outputFilePath), 0o755); err != nil {
			return err
		}

		outputFile, err := os.Create(outputFilePath)
		if err != nil {
			return err
		}

		defer outputFile.Close()

		r, err := info.Open()
		if err != nil {
			return err
		}

		defer r.Close()

		if _, err := io.Copy(outputFile, r); err != nil {
			return err
		}

		return nil
	}
}

func (c *ConfigContext) AddArtifactSource(sourceName string, source artifact.ArtifactSource) (*artifactApi.ArtifactSourceId, error) {
	// 1. If source is cached using '<source-name>-<digest>', return the source id

	// TODO: if any paths are relative, they should be expanded to the _artifact's source directory

	// 1a. Determine source kind

	sourcePathKind := artifact.ArtifactSourceKind_UNKNOWN

	if _, err := os.Stat(source.Path); err == nil {
		sourcePathKind = artifact.ArtifactSourceKind_LOCAL
	}

	if strings.HasPrefix(source.Path, "git://") {
		sourcePathKind = artifact.ArtifactSourceKind_GIT
	}

	if strings.HasPrefix(source.Path, "http://") || strings.HasPrefix(source.Path, "https://") {
		sourcePathKind = artifact.ArtifactSourceKind_HTTP
	}

	if sourcePathKind == artifact.ArtifactSourceKind_UNKNOWN {
		return nil, fmt.Errorf("`source.%s.path` unknown kind: %v", sourceName, source.Path)
	}

	// 1b. process source path

	if sourcePathKind == artifact.ArtifactSourceKind_GIT {
		return nil, fmt.Errorf("`source.%s.path` git not supported", sourceName)
	}

	if sourcePathKind == artifact.ArtifactSourceKind_LOCAL {
		path, err := filepath.Abs(source.Path)
		if err != nil {
			return nil, err
		}

		source.Path = path

		// TODO: add logging library like in rust code
	}

	// 1c. process source id

	sourceJsonBytes, err := json.Marshal(source)
	if err != nil {
		return nil, err
	}

	sourceKey := fmt.Sprintf("%s-%x", sourceName, sha256.Sum256(sourceJsonBytes))

	if _, ok := c.artifactSourceId[sourceKey]; ok {
		return c.artifactSourceId[sourceKey], nil
	}

	// 2. Check if source exists in registry or local cache

	// TODO: check if source is also an empty value

	if source.Hash != nil {
		// 2a. Check if source exists in the registry

		// TODO: put client at higher level with connection pooling

		registryConnOpts := []grpc.DialOption{}

		registryConn, err := grpc.NewClient(c.registry, registryConnOpts...)
		if err != nil {
			return nil, err
		}

		defer registryConn.Close()

		registryClient := registryApi.NewRegistryServiceClient(registryConn)

		registryRequest := &registryApi.RegistryRequest{
			Hash: *source.Hash,
			Kind: registryApi.RegistryKind_ARTIFACT_SOURCE,
			Name: sourceName,
		}

		registryResponse, err := registryClient.Exists(context.Background(), registryRequest)
		if err != nil && status.Code(err) != codes.NotFound {
			return nil, err
		}

		if registryResponse.Success {
			return &artifactApi.ArtifactSourceId{
				Hash: *source.Hash,
				Name: sourceName,
			}, nil
		}

		// 2b. Check if source exists in local cache

		sourceCacheArchivePath := store.GetCacheArchivePath(*source.Hash, sourceName)

		if _, err := os.Stat(sourceCacheArchivePath); err == nil {
			return &artifactApi.ArtifactSourceId{
				Hash: *source.Hash,
				Name: sourceName,
			}, nil
		}
	}

	// 3. Prepare source if not cached

	sourceSandboxPath, err := store.NewSandboxDir()
	if err != nil {
		return nil, err
	}

	if sourcePathKind == artifact.ArtifactSourceKind_HTTP {
		if source.Hash == nil {
			return nil, fmt.Errorf("`source.%s.hash` required for remote source", sourceName)
		}

		if source.Hash != nil && *source.Hash == "" {
			return nil, fmt.Errorf("`source.%s.hash` cannot be empty for remote source", sourceName)
		}

		// 3a. Download source

		sourceRemotePath, err := url.Parse(source.Path)
		if err != nil {
			return nil, err
		}

		if sourceRemotePath.Scheme != "http" && sourceRemotePath.Scheme != "https" {
			return nil, fmt.Errorf("`source.%s.path` must be http or https", sourceName)
		}

		sourceResponse, err := http.Get(sourceRemotePath.String())
		if err != nil {
			return nil, err
		}

		defer sourceResponse.Body.Close()

		if sourceResponse.StatusCode != http.StatusOK {
			return nil, fmt.Errorf("HTTP request failed: %s", sourceResponse.Status)
		}

		sourceContentType, err := filetype.MatchReader(sourceResponse.Body)
		if err != nil {
			return nil, err
		}

		if sourceContentType == filetype.Unknown {
			sourceFileName := filepath.Base(sourceRemotePath.Path)

			sourceFilePath := filepath.Join(*sourceSandboxPath, sourceFileName)

			sourceFile, err := os.Create(sourceFilePath)
			if err != nil {
				return nil, err
			}

			defer sourceFile.Close()

			_, err = io.Copy(sourceFile, sourceResponse.Body)
			if err != nil {
				return nil, err
			}
		}

		// 3b. Extract source

		ctx := context.Background()

		switch sourceContentType.MIME.Value {
		case "application/gzip":
			decoder := archives.Gz{}

			decoderReader, err := decoder.OpenReader(sourceResponse.Body)
			if err != nil {
				return nil, err
			}

			defer decoderReader.Close()

			archive := archives.Tar{}

			err = archive.Extract(ctx, decoderReader, handleFile(*sourceSandboxPath))

		case "application/x-bzip2":
			decoder := archives.Bz2{}

			decoderReader, err := decoder.OpenReader(sourceResponse.Body)
			if err != nil {
				return nil, err
			}

			defer decoderReader.Close()

			archive := archives.Tar{}

			err = archive.Extract(ctx, decoderReader, handleFile(*sourceSandboxPath))

		case "application/x-xz":
			decoder := archives.Xz{}

			decoderReader, err := decoder.OpenReader(sourceResponse.Body)
			if err != nil {
				return nil, err
			}

			defer decoderReader.Close()

			archive := archives.Tar{}

			err = archive.Extract(ctx, decoderReader, handleFile(*sourceSandboxPath))

		case "application/zip":
			archive := archives.Zip{}

			err = archive.Extract(ctx, sourceResponse.Body, handleFile(*sourceSandboxPath))

		default:
			return nil, fmt.Errorf("unsupported content type: %s", sourceContentType.MIME.Value)
		}

		if err != nil {
			return nil, err
		}
	}

	if sourcePathKind == artifact.ArtifactSourceKind_LOCAL {
		sourcePaths, err := store.GetFilePaths(source.Path, source.Excludes, source.Includes)
		if err != nil {
			return nil, err
		}

		_, err = store.CopyFiles(source.Path, sourcePaths, *sourceSandboxPath)
		if err != nil {
			return nil, err
		}
	}

	// 4. Calculate source hash

	sourceSandboxFiles, err := store.GetFilePaths(*sourceSandboxPath, source.Excludes, source.Includes)
	if err != nil {
		return nil, err
	}

	if len(sourceSandboxFiles) == 0 {
		return nil, fmt.Errorf("Artifact `source.%s.path` no files found: %s", sourceName, source.Path)
	}

	// 4a. Set timestamps

	for _, file := range sourceSandboxFiles {
		err := store.SetTimestamps(file)
		if err != nil {
			return nil, err
		}
	}

	// 4b. Hash source files

	sourceHash, err := store.HashFiles(sourceSandboxFiles)
	if err != nil {
		return nil, err
	}

	if source.Hash != nil && *source.Hash != sourceHash {
		return nil, fmt.Errorf("`source.%s.hash` mismatch: %s != %s", sourceName, sourceHash, *source.Hash)
	}

	// 4c. Cache source

	ctx := context.Background()

	sourceCacheArchiveFiles := make(map[string]string)

	for _, file := range sourceSandboxFiles {
		relPath, err := filepath.Rel(*sourceSandboxPath, file)
		if err != nil {
			return nil, err
		}

		sourceCacheArchiveFiles[file] = relPath
	}

	files, err := archives.FilesFromDisk(ctx, nil, sourceCacheArchiveFiles)
	if err != nil {
		return nil, err
	}

	sourceCacheArchivePath := store.GetCacheArchivePath(sourceHash, sourceName)

	out, err := os.Create(sourceCacheArchivePath)
	if err != nil {
		return nil, err
	}

	defer out.Close()

	format := archives.CompressedArchive{
		Archival:    archives.Tar{},
		Compression: archives.Zstd{},
	}

	err = format.Archive(ctx, out, files)
	if err != nil {
		return nil, err
	}

	err = os.RemoveAll(*sourceSandboxPath)
	if err != nil {
		return nil, err
	}

	sourceId := &artifactApi.ArtifactSourceId{
		Hash: sourceHash,
		Name: sourceName,
	}

	c.artifactSourceId[sourceKey] = sourceId

	return sourceId, nil
}

func (c *ConfigContext) AddArtifact(name string, artifacts []*artifactApi.ArtifactId, sources []*artifactApi.ArtifactSourceId, steps []*artifactApi.ArtifactStep, systems []string) (*artifactApi.ArtifactId, error) {
	// 1. Setup systems

	systemsInt := make([]artifactApi.ArtifactSystem, len(systems))

	for i, system := range systems {
		systemType := artifact.GetArtifactSystem(system)
		if systemType == artifactApi.ArtifactSystem_UNKNOWN_SYSTEM {
			return nil, fmt.Errorf("Unsupported system: %s", system)
		}

		systemsInt[i] = systemType
	}

	// 2. Setup artifact id

	artifact := &artifactApi.Artifact{
		Artifacts: artifacts,
		Name:      name,
		Sources:   sources,
		Steps:     steps,
		Systems:   systemsInt,
	}

	artifactManifest := &artifactApi.ArtifactBuildRequest{
		Artifact: artifact,
		System:   c.system,
	}

	artifactManifestBytes, err := json.Marshal(artifactManifest)
	if err != nil {
		return nil, err
	}

	artifactManifestHash := fmt.Sprintf("%x", sha256.Sum256(artifactManifestBytes))

	artifactId := &artifactApi.ArtifactId{
		Hash: artifactManifestHash,
		Name: name,
	}

	if _, ok := c.ArtifactId[artifactId]; !ok {
		c.ArtifactId[artifactId] = artifact
	}

	return artifactId, nil
}

func (c *ConfigContext) GetArtifact(hash string, name string) (*artifactApi.Artifact, error) {
	artifactId := &artifactApi.ArtifactId{
		Hash: hash,
		Name: name,
	}

	// NOTE: this may be a bug as it requires a pointer to the artifactId
	// instead, it may require a loop to find the artifactId

	artifact, ok := c.ArtifactId[artifactId]
	if !ok {
		return nil, fmt.Errorf("Artifact not found: %s", name)
	}

	return artifact, nil
}

func (c *ConfigContext) GetTarget() artifactApi.ArtifactSystem {
	return c.system
}

func (c *ConfigContext) Run(artifacts []*artifactApi.ArtifactId) error {
	listener, err := net.Listen("tcp", fmt.Sprintf("[::]:%d", c.port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	config := &configApi.Config{
		Artifacts: artifacts,
	}

	var opts []grpc.ServerOption

	grpcServer := grpc.NewServer(opts...)

	configApi.RegisterConfigServiceServer(grpcServer, NewConfigServer(c, config))

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}

func (s *ConfigServer) GetConfig(ctx context.Context, request *configApi.ConfigRequest) (*configApi.Config, error) {
	return s.Config, nil
}

func (s *ConfigServer) GetArtifact(ctx context.Context, request *artifactApi.ArtifactId) (*artifactApi.Artifact, error) {
	artifact, err := s.Context.GetArtifact(request.Hash, request.Name)
	if err != nil {
		return nil, err
	}

	if artifact == nil {
		return nil, fmt.Errorf("Artifact not found: %s", request.Name)
	}

	return artifact, nil
}
