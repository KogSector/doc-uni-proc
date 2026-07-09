# Unified Processor

**Port**: 8090

A high-performance document and code processing service that builds source-specific structural knowledge graphs.

## Overview

The Unified Processor handles all file processing tasks for the ConFuse platform. Crucially, it operates on a **Strict Same-Source Policy**: It processes one source (a repository, a document upload, a URL crawl) at a time, and *only* extracts relationships that exist within that single source. 

- **Code Processing**: Parse, normalize, and chunk code files
- **Document Processing**: Extract text from documents and chunk appropriately  
- **Language Detection**: Automatically detect programming languages and document types
- **Intelligent Chunking**: Context-aware text chunking for optimal embeddings
- **Metadata Extraction**: Extract rich metadata from processed files
- **Kafka Integration**: Publish processing events to downstream services
- **Graph Storage**: Directly writes nodes and structural edges to FalkorDB

*Note: Cross-source relationship discovery is delegated to the `fast-fetcher` microservice.*

## Features

- **Unified Architecture**: Single service handles both code and document processing
- **Rust Performance**: High-performance core with Python ML integration
- **FalkorDB Storage**: Native vector search and property graph storage
- **PostgreSQL Integration**: Track processing jobs and file metadata
- **Multi-format Support**: Python, JavaScript, TypeScript, Java, Go, Rust, PDF, Word, Markdown

## Quick Start

### Prerequisites

- Rust 1.70+
- Python 3.11+
- FalkorDB (via Redis protocol)
- PostgreSQL
- Kafka

### Installation

```bash
# Install Rust dependencies
cargo build --release

# Install Python dependencies
pip install -r requirements.txt
```

### Configuration

Set environment variables:

```bash
export DATABASE_URL="postgresql://localhost/unified_processor"
export KAFKA_BOOTSTRAP_SERVERS="localhost:9092"
export FALKORDB_HOST="localhost"
export FALKORDB_PORT="6379"
export FALKORDB_PASSWORD="your-password"
export PORT=8090
```

### Running

```bash
# Development
cargo run

# Production
cargo run --release
```

## API Endpoints

### Health Check
```bash
GET /health
```

### Process Files
```bash
POST /api/v1/process
{
  "source_id": "github-user-repo",
  "files": [
    {
      "path": "src/main.rs",
      "content": "fn main() { println!(\"Hello\"); }",
      "type": "code"
    }
  ],
  "metadata": {
    "force_reprocess": true
  }
}
```

### Streaming Repository Ingestion
```bash
POST /api/v1/repo/ingest/stream
{
  "provider": "github",
  "owner": "username",
  "repo": "repository",
  "branch": "main",
  "file_extensions": [".py", ".rs", ".js"]
}
```

## Architecture

```
unified-processor/
├── src/
│   ├── core/           # Configuration and error handling
│   ├── processing/     # File processing logic
│   ├── chunking/       # Text chunking strategies
│   ├── storage/        # Database operations (FalkorDB + PostgreSQL)
│   ├── search/         # Vector search with FalkorDB
│   ├── infra/          # Infrastructure clients (gRPC, etc.)
│   │   └── grpc/       # gRPC clients for embeddings-service
│   ├── events/         # Kafka event handling
│   ├── proto/          # Generated protobuf definitions (gRPC server)
│   ├── graph/          # Graph extractors and resolvers
│   └── models/         # Data structures
├── proto/             # Protobuf schema definitions
├── src/utils/         # Python ML models and NLP
├── build.rs           # Build script for protobuf code generation
├── Cargo.toml         # Rust dependencies
├── requirements.txt    # Python dependencies
└── Dockerfile         # Container configuration
```

## Integration

The unified processor integrates with:

- **data-connector**: Receives file processing requests via HTTP and Kafka
- **embeddings-service**: Generates embeddings via gRPC client and publishes chunk events
- **auth-middleware**: Validates processing requests
- **fast-fetcher**: Discovers cross-source relationships between graphs created here
- **FalkorDB**: Stores vector embeddings with native vector search support

### Processing with Embeddings

1. **Chunk Processing**: Unified processor creates text chunks
2. **gRPC Call**: Batch embedding generation via embeddings-service
3. **Vector Storage**: Store embeddings in FalkorDB Vector_Chunk nodes
4. **Relationship Creation**: Create Document→Vector_Chunk and structural sequential/hierarchical relationships
5. **Event Publishing**: Notify downstream services (like `fast-fetcher`) of completion

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PORT` | No | 8090 | Server port |
| `DATABASE_URL` | Yes | - | PostgreSQL connection URL |
| `KAFKA_BOOTSTRAP_SERVERS` | Yes | - | Kafka bootstrap servers |
| `KAFKA_ENABLED` | No | true | Enable Kafka integration |
| `AUTH_SERVICE_URL` | No | http://auth-middleware:3010 | Auth service URL |
| `EMBEDDINGS_SERVICE_URL` | No | https://embeddings-service-xmg6.onrender.com | Embeddings service gRPC endpoint |
| `FALKORDB_HOST` | No | localhost | FalkorDB host |
| `FALKORDB_PORT` | No | 6379 | FalkorDB port |
| `FALKORDB_USERNAME` | No | neo4j | FalkorDB username |
| `FALKORDB_PASSWORD` | Yes | - | FalkorDB password |
| `FALKORDB_GRAPH_NAME` | No | knowledge-layer | FalkorDB graph name |

## Documentation
- [Architecture Details](docs/ARCHITECTURE.md)

## License

MIT - ConFuse Team
