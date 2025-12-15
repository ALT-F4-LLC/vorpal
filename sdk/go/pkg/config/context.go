package config

import (
	"context"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"strings"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/agent"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	apiContext "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/context"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/metadata"
)

type ArtifactAlias struct {
	Name      string
	Namespace string
	Tag       string
}

type ConfigContextStore struct {
	artifact map[string]*artifact.Artifact
	variable map[string]string
}

type ConfigContext struct {
	artifact          string
	artifactContext   string
	artifactNamespace string
	artifactSystem    artifact.ArtifactSystem
	artifactUnlock    bool
	clientAgent       agent.AgentServiceClient
	clientArtifact    artifact.ArtifactServiceClient
	port              int
	registry          string
	store             ConfigContextStore
}

// VorpalCredentialsContent represents OIDC credentials for an issuer
type VorpalCredentialsContent struct {
	AccessToken  string   `json:"access_token"`
	ExpiresIn    int64    `json:"expires_in"`
	RefreshToken string   `json:"refresh_token"`
	Scopes       []string `json:"scopes"`
}

// VorpalCredentials represents the credentials file structure
type VorpalCredentials struct {
	Issuer   map[string]VorpalCredentialsContent `json:"issuer"`
	Registry map[string]string                   `json:"registry"`
}

type ConfigServer struct {
	apiContext.UnimplementedContextServiceServer

	store ConfigContextStore
}

func NewConfigServer(store ConfigContextStore) *ConfigServer {
	return &ConfigServer{
		store: store,
	}
}

func (s *ConfigServer) GetArtifact(ctx context.Context, request *artifact.ArtifactRequest) (*artifact.Artifact, error) {
	if request.Digest == "" {
		return nil, fmt.Errorf("'digest' is required")
	}

	response := s.store.artifact[request.Digest]
	if response == nil {
		return nil, fmt.Errorf("artifact not found")
	}

	return response, nil
}

func (s *ConfigServer) GetArtifacts(ctx context.Context, request *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error) {
	digests := make([]string, 0)

	for digest := range s.store.artifact {
		digests = append(digests, digest)
	}

	response := &artifact.ArtifactsResponse{
		Digests: digests,
	}

	return response, nil
}

// ClientAuthHeader retrieves the authorization header for a given registry.
// Returns the Bearer token string if credentials exist, empty string otherwise, or error on failure.
// This matches the Rust SDK's client_auth_header function.
func ClientAuthHeader(registry string) (string, error) {
	credentialsPath := GetKeyCredentialsPath()

	// Check if credentials file exists (like Rust's .exists())
	if _, err := os.Stat(credentialsPath); os.IsNotExist(err) {
		// No credentials file - return empty string (optional auth)
		return "", nil
	}

	// Read credentials file
	credentialsData, err := os.ReadFile(credentialsPath)
	if err != nil {
		return "", fmt.Errorf("failed to read credentials file: %w", err)
	}

	// Parse JSON
	var credentials VorpalCredentials
	if err := json.Unmarshal(credentialsData, &credentials); err != nil {
		return "", fmt.Errorf("failed to parse credentials: %w", err)
	}

	// Lookup registry -> issuer mapping
	registryIssuer, ok := credentials.Registry[registry]
	if !ok {
		return "", fmt.Errorf("no issuer found for registry: %s", registry)
	}

	// Lookup issuer credentials
	issuerCredentials, ok := credentials.Issuer[registryIssuer]
	if !ok {
		return "", fmt.Errorf("no issuer found for registry: %s (issuer: %s)", registry, registryIssuer)
	}

	// Format Bearer token
	return fmt.Sprintf("Bearer %s", issuerCredentials.AccessToken), nil
}

func GetContext() *ConfigContext {
	cmd, err := NewCommand()
	if err != nil {
		log.Fatal(err)
	}

	store := ConfigContextStore{
		artifact: make(map[string]*artifact.Artifact),
		variable: cmd.ArtifactVariable,
	}

	system, err := GetSystem(cmd.ArtifactSystem)
	if err != nil {
		log.Fatalf("failed to get system: %v", err)
	}

	// Auth headers will be added per-request using ClientAuthHeader

	caCert, err := os.ReadFile("/var/lib/vorpal/key/ca.pem")
	if err != nil {
		log.Fatalf("Failed to read CA certificate: %v", err)
	}

	caCertPool := x509.NewCertPool()
	if !caCertPool.AppendCertsFromPEM(caCert) {
		log.Fatal("Failed to append CA certificate")
	}

	credentials := credentials.NewTLS(&tls.Config{
		RootCAs:    caCertPool,
		ServerName: "localhost",
	})

	agentHost := strings.ReplaceAll(cmd.Agent, "https://", "")

	clientConnAgent, err := grpc.NewClient(
		agentHost,
		grpc.WithTransportCredentials(credentials),
	)
	if err != nil {
		log.Fatalf("failed to connect to agent: %v", err)
	}

	registryHost := strings.ReplaceAll(cmd.Registry, "https://", "")

	clientConnArtifact, err := grpc.NewClient(
		registryHost,
		grpc.WithTransportCredentials(credentials),
	)
	if err != nil {
		log.Fatalf("failed to connect to agent: %v", err)
	}

	return &ConfigContext{
		artifact:          cmd.Artifact,
		artifactContext:   cmd.ArtifactContext,
		artifactNamespace: cmd.ArtifactNamespace,
		artifactSystem:    *system,
		artifactUnlock:    cmd.ArtifactUnlock,
		clientAgent:       agent.NewAgentServiceClient(clientConnAgent),
		clientArtifact:    artifact.NewArtifactServiceClient(clientConnArtifact),
		port:              cmd.Port,
		registry:          cmd.Registry,
		store:             store,
	}
}

func (c *ConfigContext) AddArtifact(artifact *artifact.Artifact) (*string, error) {
	if artifact.Name == "" {
		return nil, fmt.Errorf("'name' is required")
	}

	if len(artifact.Steps) == 0 {
		return nil, fmt.Errorf("'steps' is required")
	}

	if len(artifact.Systems) == 0 {
		return nil, fmt.Errorf("'systems' is required")
	}

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

	prepareRequest := &agent.PrepareArtifactRequest{
		Artifact:          artifact,
		ArtifactContext:   c.artifactContext,
		ArtifactNamespace: c.artifactNamespace,
		ArtifactUnlock:    c.artifactUnlock,
		Registry:          c.registry,
	}

	// Get auth header for this registry
	authHeader, err := ClientAuthHeader(c.registry)
	if err != nil {
		return nil, fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	clientResponse, err := c.clientAgent.PrepareArtifact(ctx, prepareRequest)
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

func fetchArtifacts(client artifact.ArtifactServiceClient, digest string, namespace string, store map[string]*artifact.Artifact, registry string) error {
	if _, ok := store[digest]; ok {
		return nil
	}

	// Get auth header
	authHeader, err := ClientAuthHeader(registry)
	if err != nil {
		return fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	clientResponse, err := client.GetArtifact(ctx, &artifact.ArtifactRequest{Digest: digest, Namespace: namespace})
	if err != nil {
		return fmt.Errorf("error fetching artifact: %v", err)
	}

	if _, ok := store[digest]; !ok {
		store[digest] = clientResponse
	}

	for _, step := range clientResponse.Steps {
		if step != nil {
			for _, digest := range step.Artifacts {
				fetchArtifacts(client, digest, namespace, store, registry)
			}
		}
	}

	return nil
}

// parseArtifactAlias parses an artifact alias into its components.
// Format: [<namespace>/]<name>[:<tag>]
// - namespace is optional (defaults to "library")
// - tag is optional (defaults to "latest")
// - name is required
func parseArtifactAlias(alias string) (*ArtifactAlias, error) {
	// Validate input
	if alias == "" {
		return nil, fmt.Errorf("alias cannot be empty")
	}

	if len(alias) > 255 {
		return nil, fmt.Errorf("alias too long (max 255 characters)")
	}

	// Step 1: Extract tag (split on rightmost ':')
	tag := ""
	base := alias

	if lastColon := strings.LastIndex(alias, ":"); lastColon != -1 {
		tagPart := alias[lastColon+1:]
		if tagPart == "" {
			return nil, fmt.Errorf("tag cannot be empty")
		}
		tag = tagPart
		base = alias[:lastColon]
	}

	// Step 2: Extract namespace/name (split on '/')
	namespace := ""
	name := ""

	slashCount := strings.Count(base, "/")

	switch slashCount {
	case 0:
		// Just name
		name = base
	case 1:
		// namespace/name
		slashIdx := strings.Index(base, "/")
		namespace = base[:slashIdx]
		name = base[slashIdx+1:]

		if namespace == "" {
			return nil, fmt.Errorf("namespace cannot be empty")
		}
	default:
		// Too many slashes
		return nil, fmt.Errorf("invalid format: too many path separators")
	}

	if name == "" {
		return nil, fmt.Errorf("name is required")
	}

	// Step 3: Apply defaults
	if tag == "" {
		tag = "latest"
	}

	if namespace == "" {
		namespace = "library"
	}

	return &ArtifactAlias{
		Name:      name,
		Namespace: namespace,
		Tag:       tag,
	}, nil
}

func (c *ConfigContext) FetchArtifactAlias(alias string) (*string, error) {
	parsed, err := parseArtifactAlias(alias)
	if err != nil {
		return nil, fmt.Errorf("failed to parse artifact alias: %w", err)
	}

	request := &artifact.GetArtifactAliasRequest{
		Name:      parsed.Name,
		Namespace: parsed.Namespace,
		System:    c.artifactSystem,
		Tag:       parsed.Tag,
	}

	// Get auth header for this registry
	authHeader, err := ClientAuthHeader(c.registry)
	if err != nil {
		return nil, fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	response, err := c.clientArtifact.GetArtifactAlias(ctx, request)
	if err != nil {
		return nil, fmt.Errorf("error fetching artifact alias: %v", err)
	}

	artifactDigest := response.Digest

	if _, ok := c.store.artifact[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	err = fetchArtifacts(c.clientArtifact, artifactDigest, parsed.Namespace, c.store.artifact, c.registry)
	if err != nil {
		return nil, fmt.Errorf("error fetching '%s': %v", artifactDigest, err)
	}

	return &artifactDigest, nil
}

func (c *ConfigContext) GetArtifact(digest string) *artifact.Artifact {
	return c.store.artifact[digest]
}

func (c *ConfigContext) GetArtifactContextPath() string {
	return c.artifactContext
}

func (c *ConfigContext) GetArtifactName() string {
	return c.artifact
}

func (c *ConfigContext) GetTarget() artifact.ArtifactSystem {
	return c.artifactSystem
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

	apiContext.RegisterContextServiceServer(grpcServer, NewConfigServer(c.store))

	listenerAddr := fmt.Sprintf("[::]:%d", c.port)

	listener, err := net.Listen("tcp", listenerAddr)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	log.Printf("context service: %s", listenerAddr)

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}
