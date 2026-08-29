# Vercel Environment Variables Setup Guide

## Overview

This document provides a comprehensive guide for configuring all required and optional environment variables for deploying the doc-uni-proc service on Vercel. The service is a Rust + Python hybrid microservice that requires proper configuration of database, graph database, message queue, authentication, and optional AI model endpoints.

## Table of Contents

- [Required Environment Variables](#required-environment-variables)
- [Optional Environment Variables](#optional-environment-variables)
- [Variable Sources](#variable-sources)
- [Vercel Dashboard Setup Instructions](#vercel-dashboard-setup-instructions)
- [Local Testing](#local-testing)
- [Troubleshooting](#troubleshooting)

---

## Required Environment Variables

These environment variables **must** be configured in Vercel for the service to start successfully. Missing any of these will cause the service to fail during startup with a descriptive error message.

### Database Configuration

| Variable | Description | Example Value | Source File |
|----------|-------------|---------------|-------------|
| `DATABASE_URL` or `POSTGRES_URL` | PostgreSQL connection string for metadata storage. The service accepts either variable name. | `postgresql://user:pass@host:5432/db?sslmode=verify-full` | `.env.secret` |

**Note**: Either `DATABASE_URL` or `POSTGRES_URL` must be set. The service will use whichever is available.

### FalkorDB Configuration

FalkorDB is used for graph and vector storage. All five variables below are required:

| Variable | Description | Example Value | Source File |
|----------|-------------|---------------|-------------|
| `FALKORDB_HOST` | FalkorDB instance hostname | `r-6jissuruar.instance-ivah2xvml.hc-7up0crkyn.ap-south-1.aws.f2e0a955bb84.cloud` | `.env.map` |
| `FALKORDB_PORT` | FalkorDB instance port | `50860` | `.env.map` |
| `FALKORDB_USERNAME` | FalkorDB authentication username | `adminconfuse` | `.env.map` |
| `FALKORDB_PASSWORD` | FalkorDB authentication password | `graph4confuse` | `.env.secret` |
| `FALKORDB_USE_TLS` | Enable/disable TLS for FalkorDB connection | `false` | `.env.map` |

### Kafka Configuration

Kafka is used for event streaming. The following variables are required:

| Variable | Description | Example Value | Source File |
|----------|-------------|---------------|-------------|
| `KAFKA_BOOTSTRAP_SERVERS` | Kafka broker addresses (comma-separated) | `confuse-kafka-confuse-kafka-setups.i.aivencloud.com:26443` | `.env.map` |
| `KAFKA_SASL_USERNAME` | Kafka SASL authentication username | `avnadmin` | `.env.map` |
| `KAFKA_SASL_PASSWORD` | Kafka SASL authentication password | `AVNS_zEk3sH4h5ZZe1Dca90F` | `.env.secret` |

**Optional Kafka Variables** (defaults shown):

| Variable | Description | Default Value | Source File |
|----------|-------------|---------------|-------------|
| `KAFKA_SECURITY_PROTOCOL` | Security protocol for Kafka connection | `SASL_SSL` | `.env.map` |
| `KAFKA_SASL_MECHANISM` | SASL authentication mechanism | `PLAIN` | `.env.map` |
| `KAFKA_SSL_CA_PEM` | Kafka SSL CA certificate (PEM format) | *(See `.env.secret` for full cert)* | `.env.secret` |

### Authentication Configuration

| Variable | Description | Example Value | Source File |
|----------|-------------|---------------|-------------|
| `AUTH_MIDDLEWARE_URL` | URL of the authentication middleware service | `https://auth.confuse.site` | `.env.map` |
| `INTERNAL_API_KEY` | API key for internal service-to-service authentication | `b29c48f7a63d91e5c024f8d3b71a6e9f2d5c8b4a1e3f7d9c0b2a5e8f1d4c7b6a` | `.env.map` |

---

## Optional Environment Variables

These variables enable additional features but are not required for basic service functionality.

### NVIDIA NIM Configuration

NVIDIA NIM (Neural Inference Microservice) provides optional OCR (Optical Character Recognition) capabilities using vision-language models. Configure these variables if you want to enable advanced document image processing:

| Variable | Description | Example Value | Source File |
|----------|-------------|---------------|-------------|
| `NVIDIA_NIM_ENDPOINT` | NVIDIA NIM API endpoint URL | `https://integrate.api.nvidia.com/v1` | `.env.map` |
| `NVIDIA_NIM_MODEL` | Model identifier for OCR processing | `meta/muse-glimmer-30b` | `.env.map` |
| `NVIDIA_NIM_API_KEY` | API key for NVIDIA NIM authentication | `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` | `.env.secret` |

**Additional NIM Configuration** (currently commented out in `.env.map`):

| Variable | Description | Default Value |
|----------|-------------|---------------|
| `NIM_BATCH_SIZE` | Batch size for NIM inference requests | `4` |
| `NIM_TIMEOUT_SECS` | Timeout for NIM API requests (seconds) | `120` |
| `NIM_MAX_TOKENS` | Maximum tokens for NIM model output | `4096` |

---

## Variable Sources

The environment variables are organized into two files in the repository:

### `.env.map` (Non-Secret Configuration)

This file contains **non-sensitive** configuration values that can be safely committed to version control:

- Service URLs (`AUTH_MIDDLEWARE_URL`, `NVIDIA_NIM_ENDPOINT`)
- Hostnames and ports (`FALKORDB_HOST`, `FALKORDB_PORT`, `KAFKA_BOOTSTRAP_SERVERS`)
- Usernames (`FALKORDB_USERNAME`, `KAFKA_SASL_USERNAME`)
- Protocol settings (`KAFKA_SECURITY_PROTOCOL`, `KAFKA_SASL_MECHANISM`)
- Model identifiers (`NVIDIA_NIM_MODEL`)
- Public API keys that are meant to be visible (`INTERNAL_API_KEY`)

### `.env.secret` (Secret Configuration)

This file contains **sensitive** credentials and secrets that must **never** be committed to version control (it should be in `.gitignore`):

- Database connection strings with passwords (`POSTGRES_URL`)
- Authentication passwords (`FALKORDB_PASSWORD`, `KAFKA_SASL_PASSWORD`)
- API keys for external services (`NVIDIA_NIM_API_KEY`)
- SSL/TLS certificates (`KAFKA_SSL_CA_PEM`)

**Security Note**: Always store secrets using Vercel's encrypted environment variable storage, never commit them to your repository.

---

## Vercel Dashboard Setup Instructions

Follow these steps to configure environment variables in Vercel:

### Step 1: Navigate to Project Settings

1. Log in to [Vercel Dashboard](https://vercel.com/dashboard)
2. Select your project (e.g., `doc-uni-proc`)
3. Click **Settings** in the top navigation bar
4. Select **Environment Variables** from the left sidebar

### Step 2: Add Required Variables

For each required variable, click **Add New** and fill in:

- **Key**: Variable name (e.g., `DATABASE_URL`)
- **Value**: Variable value (copy from `.env.map` or `.env.secret`)
- **Environment**: Select the environments where this variable should be available:
  - ✅ **Production** (required for production deployments)
  - ✅ **Preview** (recommended for testing preview deployments)
  - ✅ **Development** (optional, for local development via `vercel dev`)

### Step 3: Database Configuration

Add the PostgreSQL connection string:

```
Key: POSTGRES_URL
Value: postgresql://neondb_owner:npg_nwMBCeG2rpW5@ep-cold-band-azixxknu-pooler.c-3.ap-southeast-1.aws.neon.tech/neondb?sslmode=verify-full&channel_binding=require
Environments: Production, Preview
```

**Alternative**: You can also use `DATABASE_URL` instead of `POSTGRES_URL` - the service accepts either name.

### Step 4: FalkorDB Configuration

Add all five FalkorDB variables:

```
Key: FALKORDB_HOST
Value: r-6jissuruar.instance-ivah2xvml.hc-7up0crkyn.ap-south-1.aws.f2e0a955bb84.cloud
Environments: Production, Preview

Key: FALKORDB_PORT
Value: 50860
Environments: Production, Preview

Key: FALKORDB_USERNAME
Value: adminconfuse
Environments: Production, Preview

Key: FALKORDB_PASSWORD
Value: graph4confuse
Environments: Production, Preview

Key: FALKORDB_USE_TLS
Value: false
Environments: Production, Preview
```

### Step 5: Kafka Configuration

Add the three required Kafka variables:

```
Key: KAFKA_BOOTSTRAP_SERVERS
Value: confuse-kafka-confuse-kafka-setups.i.aivencloud.com:26443
Environments: Production, Preview

Key: KAFKA_SASL_USERNAME
Value: avnadmin
Environments: Production, Preview

Key: KAFKA_SASL_PASSWORD
Value: AVNS_zEk3sH4h5ZZe1Dca90F
Environments: Production, Preview
```

**Optional Kafka Variables**:

```
Key: KAFKA_SECURITY_PROTOCOL
Value: SASL_SSL
Environments: Production, Preview

Key: KAFKA_SASL_MECHANISM
Value: PLAIN
Environments: Production, Preview

Key: KAFKA_SSL_CA_PEM
Value: (Copy the full PEM certificate from .env.secret)
Environments: Production, Preview
```

### Step 6: Authentication Configuration

Add authentication variables:

```
Key: AUTH_MIDDLEWARE_URL
Value: https://auth.confuse.site
Environments: Production, Preview

Key: INTERNAL_API_KEY
Value: b29c48f7a63d91e5c024f8d3b71a6e9f2d5c8b4a1e3f7d9c0b2a5e8f1d4c7b6a
Environments: Production, Preview
```

### Step 7: Optional NVIDIA NIM Configuration

If you want to enable NVIDIA NIM OCR features, add these variables:

```
Key: NVIDIA_NIM_ENDPOINT
Value: https://integrate.api.nvidia.com/v1
Environments: Production, Preview

Key: NVIDIA_NIM_MODEL
Value: meta/muse-glimmer-30b
Environments: Production, Preview

Key: NVIDIA_NIM_API_KEY
Value: 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
Environments: Production, Preview
```

### Step 8: Save and Redeploy

1. After adding all variables, click **Save** for each one
2. Vercel will prompt you to redeploy for changes to take effect
3. Click **Redeploy** or trigger a new deployment via:
   ```bash
   vercel deploy --prod
   ```

---

## Local Testing

Before deploying to Vercel, you can test the configuration locally using Docker:

### Prerequisites

- Docker installed and running
- `.env.map` and `.env.secret` files in the project root

### Build and Run Locally

```bash
# Build the Vercel-specific Docker image
docker build -f Dockerfile.vercel -t doc-uni-proc:vercel .

# Run the container with environment variables
docker run -p 8080:8080 --env-file .env.map --env-file .env.secret doc-uni-proc:vercel
```

### Test Health Check

In a separate terminal, verify the service is running:

```bash
curl http://localhost:8080/health
```

**Expected Response**:
```json
{
  "status": "healthy"
}
```

### Test with Docker Compose (Alternative)

Create a `docker-compose.yml` file:

```yaml
services:
  doc-uni-proc:
    build:
      context: .
      dockerfile: Dockerfile.vercel
    ports:
      - "8080:8080"
    env_file:
      - .env.map
      - .env.secret
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 60s
```

Then run:

```bash
docker-compose up
```

---

## Troubleshooting

### Error: "POSTGRES_URL or DATABASE_URL environment variable not set"

**Cause**: The database connection string is missing.

**Solution**: Add either `DATABASE_URL` or `POSTGRES_URL` to Vercel environment variables with your PostgreSQL connection string.

### Error: "FALKORDB_HOST environment variable not set"

**Cause**: One or more FalkorDB variables are missing.

**Solution**: Verify all five FalkorDB variables are configured:
- `FALKORDB_HOST`
- `FALKORDB_PORT`
- `FALKORDB_USERNAME`
- `FALKORDB_PASSWORD`
- `FALKORDB_USE_TLS`

### Error: "KAFKA_BOOTSTRAP_SERVERS environment variable not set"

**Cause**: Kafka configuration is incomplete.

**Solution**: Add all three required Kafka variables:
- `KAFKA_BOOTSTRAP_SERVERS`
- `KAFKA_SASL_USERNAME`
- `KAFKA_SASL_PASSWORD`

### Error: "Address already in use (os error 98)"

**Cause**: Port 8080 is already in use (local testing only).

**Solution**: Stop any other services using port 8080 or use a different port:
```bash
docker run -p 9090:8080 --env-file .env.map --env-file .env.secret doc-uni-proc:vercel
```

### Health Check Timeout

**Cause**: Service initialization is taking longer than 60 seconds.

**Possible Reasons**:
- FalkorDB connection is slow or unreachable
- Kafka broker is unreachable
- Database connection is timing out

**Solution**: Check the service logs in Vercel Dashboard:
1. Go to your deployment
2. Click **View Function Logs**
3. Look for connection errors or timeout messages
4. Verify all external services (DB, FalkorDB, Kafka) are accessible from Vercel's network

### Deployment Marked as Unhealthy

**Cause**: Health checks are failing after 3 consecutive retries.

**Solution**: 
1. Check Vercel deployment logs for startup errors
2. Verify all required environment variables are set correctly
3. Test connectivity to external services
4. If initialization takes >60 seconds, consider optimizing startup code or requesting increased timeout limits from Vercel support

### NVIDIA NIM API Errors (Optional Feature)

**Cause**: NVIDIA NIM variables are partially configured or API key is invalid.

**Solution**: Either remove all NVIDIA NIM variables (if not using OCR features) or ensure all three are correctly set:
- `NVIDIA_NIM_ENDPOINT`
- `NVIDIA_NIM_MODEL`
- `NVIDIA_NIM_API_KEY`

---

## Environment Variable Checklist

Use this checklist to ensure all variables are configured:

### Required Variables ✅

- [ ] `DATABASE_URL` or `POSTGRES_URL`
- [ ] `FALKORDB_HOST`
- [ ] `FALKORDB_PORT`
- [ ] `FALKORDB_USERNAME`
- [ ] `FALKORDB_PASSWORD`
- [ ] `FALKORDB_USE_TLS`
- [ ] `KAFKA_BOOTSTRAP_SERVERS`
- [ ] `KAFKA_SASL_USERNAME`
- [ ] `KAFKA_SASL_PASSWORD`
- [ ] `AUTH_MIDDLEWARE_URL`
- [ ] `INTERNAL_API_KEY`

### Optional Variables (NVIDIA NIM)

- [ ] `NVIDIA_NIM_ENDPOINT`
- [ ] `NVIDIA_NIM_MODEL`
- [ ] `NVIDIA_NIM_API_KEY`

### Deployment Steps

- [ ] All required variables added to Vercel Dashboard
- [ ] Variables configured for Production and Preview environments
- [ ] Local Docker build tested successfully
- [ ] Local health check returns HTTP 200
- [ ] Deployed to Vercel
- [ ] Production health check verified: `curl https://doc-uni-proc.vercel.app/health`

---

## Additional Resources

- [Vercel Environment Variables Documentation](https://vercel.com/docs/concepts/projects/environment-variables)
- [Vercel Container Deployments](https://vercel.com/docs/concepts/deployments/containers)
- [FalkorDB Documentation](https://docs.falkordb.com/)
- [Kafka Configuration Reference](https://kafka.apache.org/documentation/#configuration)
- [NVIDIA NIM API Documentation](https://docs.nvidia.com/ai/)

---

**Last Updated**: January 2026  
**Maintained By**: DevOps Team
