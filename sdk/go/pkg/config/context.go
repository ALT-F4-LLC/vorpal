package config

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"strings"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/agent"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

type ArtifactSource struct {
	Excludes []string
	Hash     *string
	Includes []string
	Path     string
}

type ArtifactSourceKind string

const (
	ArtifactSourceKind_GIT     ArtifactSourceKind = "GIT"
	ArtifactSourceKind_HTTP    ArtifactSourceKind = "HTTP"
	ArtifactSourceKind_LOCAL   ArtifactSourceKind = "LOCAL"
	ArtifactSourceKind_UNKNOWN ArtifactSourceKind = "UNKNOWN"
)

type ConfigContextStore struct {
	artifact map[string]*artifact.Artifact
	variable map[string]string
}

type ConfigContext struct {
	agent           string
	artifact        string
	artifactContext string
	port            int
	registry        string
	store           ConfigContextStore
	system          string
}

type ArtifactServer struct {
	artifact.UnimplementedArtifactServiceServer

	store ConfigContextStore
}

func NewArtifactServer(store ConfigContextStore) *ArtifactServer {
	return &ArtifactServer{
		store: store,
	}
}

func (s *ArtifactServer) GetArtifact(ctx context.Context, request *artifact.ArtifactRequest) (*artifact.Artifact, error) {
	if request.Digest == "" {
		return nil, fmt.Errorf("'digest' is required")
	}

	response := s.store.artifact[request.Digest]
	if response == nil {
		return nil, fmt.Errorf("artifact not found")
	}

	return response, nil
}

func (s *ArtifactServer) GetArtifacts(ctx context.Context, request *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error) {
	digests := make([]string, 0)

	for digest := range s.store.artifact {
		digests = append(digests, digest)
	}

	response := &artifact.ArtifactsResponse{
		Digests: digests,
	}

	return response, nil
}

func (s *ArtifactServer) StoreArtifact(ctx context.Context, request *artifact.Artifact) (*artifact.ArtifactResponse, error) {
	return nil, fmt.Errorf("not implemented")
}

func GetContext() *ConfigContext {
	cmd, err := NewCommand()
	if err != nil {
		log.Fatal(err)
	}

	store := ConfigContextStore{
		artifact: make(map[string]*artifact.Artifact),
		variable: cmd.Variable,
	}

	return &ConfigContext{
		agent:           cmd.Agent,
		artifact:        cmd.Artifact,
		artifactContext: cmd.ArtifactContext,
		port:            cmd.Port,
		registry:        cmd.Registry,
		store:           store,
		system:          cmd.System,
	}
}

func (c *ConfigContext) AddArtifact(artifact *artifact.Artifact) (*string, error) {
	// 1. Setup systems

	artifactJson, err := json.Marshal(artifact)
	if err != nil {
		return nil, err
	}

	artifactDigest := fmt.Sprintf("%x", sha256.Sum256(artifactJson))

	if _, ok := c.store.artifact[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	// TODO: make this run in parallel

	agentHost := strings.ReplaceAll(c.agent, "http://", "")

	clientConn, err := grpc.NewClient(agentHost, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	defer clientConn.Close()

	client := agent.NewAgentServiceClient(clientConn)

	clientReqest := &agent.PrepareArtifactRequest{
		Artifact:        artifact,
		ArtifactContext: c.artifactContext,
	}

	clientResponse, err := client.PrepareArtifact(context.Background(), clientReqest)
	if err != nil {
		return nil, fmt.Errorf("error preparing artifact: %v", err)
	}

	for {
		response, err := clientResponse.Recv()
		if err == io.EOF {
			break
		}

		if err != nil {
			return nil, fmt.Errorf("error receiving response: %v", err)
		}

		if response.ArtifactOutput != nil {
			output := fmt.Sprintf("%s |> %s", artifact.Name, *response.ArtifactOutput)
			println(output)
		}

		if response.Artifact != nil {
			artifact = response.Artifact
		}

		if response.ArtifactDigest != nil {
			artifactDigest = *response.ArtifactDigest
		}
	}

	if _, ok := c.store.artifact[artifactDigest]; !ok {
		c.store.artifact[artifactDigest] = artifact
	}

	return &artifactDigest, nil
}

func fetchArtifacts(client artifact.ArtifactServiceClient, digest string, store map[string]*artifact.Artifact) error {
	if _, ok := store[digest]; ok {
		return nil
	}

	clientResponse, err := client.GetArtifact(context.Background(), &artifact.ArtifactRequest{Digest: digest})
	if err != nil {
		return fmt.Errorf("error fetching artifact: %v", err)
	}

	if _, ok := store[digest]; !ok {
		store[digest] = clientResponse
	}

	for _, step := range clientResponse.Steps {
		if step != nil {
			for _, digest := range step.Artifacts {
				fetchArtifacts(client, digest, store)
			}
		}
	}

	return nil
}

func (c *ConfigContext) FetchArtifact(digest string) (*string, error) {
	if _, ok := c.store.artifact[digest]; ok {
		return &digest, nil
	}

	registry := strings.ReplaceAll(c.registry, "http://", "")

	clientConn, err := grpc.NewClient(registry, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	defer clientConn.Close()

	client := artifact.NewArtifactServiceClient(clientConn)

	err = fetchArtifacts(client, digest, c.store.artifact)
	if err != nil {
		return nil, fmt.Errorf("error fetching '%s': %v", digest, err)
	}

	return &digest, nil
}

func (c *ConfigContext) GetArtifact(digest string) *artifact.Artifact {
	return c.store.artifact[digest]
}

func (c *ConfigContext) GetArtifactName() string {
	return c.artifact
}

func (c *ConfigContext) GetTarget() (*artifact.ArtifactSystem, error) {
	system, err := GetSystem(c.system)
	if err != nil {
		return nil, fmt.Errorf("failed to get system: %w", err)
	}

	return system, nil
}

func (c *ConfigContext) GetVariable(name string) *string {
	if _, ok := c.store.variable[name]; !ok {
		return nil
	}

	value := c.store.variable[name]

	return &value
}

func (c *ConfigContext) Run() error {
	var grpcServerOpts []grpc.ServerOption

	grpcServer := grpc.NewServer(grpcServerOpts...)

	artifact.RegisterArtifactServiceServer(grpcServer, NewArtifactServer(c.store))

	listenerAddr := fmt.Sprintf("[::]:%d", c.port)

	listener, err := net.Listen("tcp", listenerAddr)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	log.Printf("artifact service: %s", listenerAddr)

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}
