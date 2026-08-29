# Deployment Checklist for doc-uni-proc

## Overview

This checklist guides you through local validation and production deployment of the doc-uni-proc service to Vercel. Follow each step in order to ensure a successful deployment.

---

## Prerequisites

- [ ] Docker installed and running locally
- [ ] Vercel CLI installed (`npm i -g vercel`)
- [ ] Access to Vercel project dashboard
- [ ] `.env.map` and `.env.secret` files available in project root
- [ ] All required environment variables documented (see `VERCEL_ENV_SETUP.md`)

---

## Phase 1: Local Docker Testing

### Step 1.1: Build the Vercel Docker Image

Build the Docker image using the Vercel-specific Dockerfile:

```bash
docker build --platform linux/amd64 -f Dockerfile.vercel -t doc-uni-proc:vercel .
```

**Expected Result**: Build completes successfully without errors. Final image size should be under 1 GB.

**Validation**:
```bash
# Check image size
docker images doc-uni-proc:vercel

# Verify image is for linux/amd64 platform
docker inspect doc-uni-proc:vercel | grep Architecture
```

**Troubleshooting**:
- If build fails with Rust compilation errors, ensure `rust-toolchain` file specifies a valid version
- If Python dependencies fail, verify `pyproject.toml` has correct package versions
- If "no space left on device" error occurs, clean up Docker: `docker system prune -a`

---

### Step 1.2: Run Container Locally

Start the container with environment variables:

```bash
docker run -p 8080:8080 \
  --env-file .env.map \
  --env-file .env.secret \
  --name doc-uni-proc-test \
  doc-uni-proc:vercel
```

**Expected Result**: Container starts and outputs initialization logs. Service should be ready within 60 seconds.

**What to Look For in Logs**:
- ✅ "Server listening on 0.0.0.0:8080"
- ✅ "Database connection established"
- ✅ "FalkorDB connection established"
- ✅ "Kafka consumer initialized"

**Troubleshooting**:
- If "POSTGRES_URL or DATABASE_URL environment variable not set", add to `.env.secret`
- If "FALKORDB_HOST environment variable not set", verify all FalkorDB vars in `.env.map`
- If "Address already in use", stop other services on port 8080 or use `-p 9090:8080`

---

### Step 1.3: Validate Health Check Endpoint

In a separate terminal, test the health endpoint:

```bash
curl -v http://localhost:8080/health
```

**Expected Response**:
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "status": "healthy"
}
```

**Alternative Test** (with timeout):
```bash
curl --max-time 10 http://localhost:8080/health
```

**Validation Criteria**:
- [ ] HTTP status code is 200
- [ ] Response received within 10 seconds
- [ ] Response body contains `"status":"healthy"`

**Troubleshooting**:
- If curl times out, check container logs for initialization errors
- If response is 503 or 500, service may still be initializing (wait up to 60 seconds)
- If connection refused, verify container is running: `docker ps`

---

### Step 1.4: Test API Endpoints (Optional)

Test a sample API endpoint to verify full functionality:

```bash
# Test document processing endpoint (requires authentication)
curl -X POST http://localhost:8080/api/v1/process \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${INTERNAL_API_KEY}" \
  -d '{"document_id": "test-123"}'
```

**Note**: Replace `${INTERNAL_API_KEY}` with the actual value from `.env.map`.

---

### Step 1.5: Clean Up Local Test

Stop and remove the test container:

```bash
docker stop doc-uni-proc-test
docker rm doc-uni-proc-test
```

---

## Phase 2: Vercel Environment Configuration

### Step 2.1: Access Vercel Dashboard

1. Log in to [Vercel Dashboard](https://vercel.com/dashboard)
2. Navigate to your project (e.g., `doc-uni-proc`)
3. Go to **Settings** → **Environment Variables**

---

### Step 2.2: Configure Required Environment Variables

Add all required variables (see `VERCEL_ENV_SETUP.md` for detailed values):

**Database Configuration**:
- [ ] `DATABASE_URL` or `POSTGRES_URL` (PostgreSQL connection string)

**FalkorDB Configuration**:
- [ ] `FALKORDB_HOST`
- [ ] `FALKORDB_PORT`
- [ ] `FALKORDB_USERNAME`
- [ ] `FALKORDB_PASSWORD`
- [ ] `FALKORDB_USE_TLS`

**Kafka Configuration**:
- [ ] `KAFKA_BOOTSTRAP_SERVERS`
- [ ] `KAFKA_SASL_USERNAME`
- [ ] `KAFKA_SASL_PASSWORD`
- [ ] `KAFKA_SECURITY_PROTOCOL` (optional, defaults to `SASL_SSL`)
- [ ] `KAFKA_SASL_MECHANISM` (optional, defaults to `PLAIN`)

**Authentication Configuration**:
- [ ] `AUTH_MIDDLEWARE_URL`
- [ ] `INTERNAL_API_KEY`

**Optional NVIDIA NIM Configuration** (if using OCR features):
- [ ] `NVIDIA_NIM_ENDPOINT`
- [ ] `NVIDIA_NIM_MODEL`
- [ ] `NVIDIA_NIM_API_KEY`

---

### Step 2.3: Verify Environment Variable Configuration

**Quick Check Command** (using Vercel CLI):

```bash
vercel env ls
```

**Expected Output**: All required variables listed with Production and Preview scopes.

---

## Phase 3: Vercel Deployment

### Step 3.1: Deploy to Vercel (Production)

Deploy using Vercel CLI:

```bash
vercel deploy --prod
```

**Alternative** (Deploy to Preview first):
```bash
# Deploy to preview environment for testing
vercel deploy

# After validation, promote to production
vercel promote <deployment-url>
```

**Expected Output**:
```
Building...
✓ Dockerfile.vercel detected
✓ Building image for linux/amd64
✓ Pushing to vcr.vercel.com/con-fuse/doc-uni-proc/dockerfile
✓ Deploying...
✓ Deployment complete

Production: https://doc-uni-proc.vercel.app
```

**Deployment Process** (Vercel will perform these automatically):
1. Detect `Dockerfile.vercel` in project root
2. Build Docker image for `linux/amd64` platform
3. Push image to Vercel container registry
4. Deploy container to Vercel edge network
5. Start health check polling (30-second intervals)
6. Mark deployment as healthy after 3 consecutive successful health checks

---

### Step 3.2: Monitor Deployment Logs

Watch real-time logs during deployment:

```bash
vercel logs --follow
```

**What to Look For**:
- ✅ "Server listening on 0.0.0.0:8080"
- ✅ "Database connection established"
- ✅ "FalkorDB connection established"
- ✅ "Kafka consumer initialized"
- ✅ Health check returning 200

**Warning Signs**:
- ❌ "POSTGRES_URL or DATABASE_URL environment variable not set"
- ❌ "Connection timeout" (FalkorDB or Kafka unreachable)
- ❌ "Health check failed" (repeated failures after start period)

---

### Step 3.3: Validate Production Health Check

Test the production health endpoint:

```bash
curl -v https://doc-uni-proc.vercel.app/health
```

**Expected Response**:
```
HTTP/2 200
Content-Type: application/json

{
  "status": "healthy"
}
```

**Validation Criteria**:
- [ ] HTTP status code is 200
- [ ] Response received within 10 seconds
- [ ] HTTPS connection established successfully
- [ ] Response body contains `"status":"healthy"`

---

### Step 3.4: Test Production API Endpoints

Verify core functionality:

```bash
# Test authenticated endpoint
curl -X POST https://doc-uni-proc.vercel.app/api/v1/process \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${INTERNAL_API_KEY}" \
  -d '{"document_id": "prod-test-123"}'
```

**Expected Result**: API responds with valid JSON (not 401 Unauthorized or 500 Internal Server Error).

---

## Phase 4: Post-Deployment Validation

### Step 4.1: Verify Vercel Dashboard Status

Check deployment status in Vercel Dashboard:

- [ ] Deployment shows green "Ready" status
- [ ] Build logs show successful Docker build
- [ ] Runtime logs show no critical errors
- [ ] Health checks are passing

---

### Step 4.2: Monitor Error Logs

Monitor for any runtime errors:

```bash
vercel logs --prod --filter=error
```

**Action Items**:
- If errors appear, investigate root cause (DB connection, Kafka, FalkorDB)
- Check that external services are accessible from Vercel's network
- Verify all environment variables are correctly set

---

### Step 4.3: Performance Baseline

Establish performance baseline:

```bash
# Test response time
curl -w "Time: %{time_total}s\n" -o /dev/null -s https://doc-uni-proc.vercel.app/health
```

**Expected Result**: Response time < 1 second (health check should be fast).

---

## Phase 5: Rollback Plan (If Needed)

### Step 5.1: Identify Previous Deployment

List recent deployments:

```bash
vercel ls
```

---

### Step 5.2: Rollback to Previous Version

If deployment fails or has critical issues:

```bash
# Promote previous stable deployment
vercel promote <previous-deployment-url>
```

**Alternative** (via Vercel Dashboard):
1. Go to **Deployments** tab
2. Find previous successful deployment
3. Click **⋯** → **Promote to Production**

---

## Common Deployment Issues

### Issue 1: Deployment Fails with "Cannot read properties of undefined (reading 'target')"

**Cause**: Vercel cannot detect `Dockerfile.vercel` file.

**Solution**:
- Verify `Dockerfile.vercel` exists in project root
- Ensure filename is exactly `Dockerfile.vercel` (case-sensitive)
- Check file is committed to git: `git status`

---

### Issue 2: Health Checks Continuously Fail

**Cause**: Service initialization takes longer than 60 seconds or fails.

**Solution**:
1. Check Vercel logs for startup errors: `vercel logs --prod`
2. Verify all environment variables are set correctly
3. Test connectivity to external services (DB, FalkorDB, Kafka)
4. If initialization legitimately takes >60 seconds, increase `--start-period` in Dockerfile.vercel health check

---

### Issue 3: Container Build Fails

**Cause**: Build errors in Rust or Python stages.

**Solution**:
1. Test build locally: `docker build -f Dockerfile.vercel -t doc-uni-proc:vercel .`
2. Check build logs for specific error messages
3. Verify all source files are committed to git
4. Ensure Cargo.toml and pyproject.toml have valid dependencies

---

### Issue 4: Image Size Exceeds Limit

**Cause**: Final Docker image is too large (>1 GB threshold).

**Solution**:
1. Check image size: `docker images doc-uni-proc:vercel`
2. Verify multi-stage build is working (see Dockerfile.vercel)
3. Ensure build artifacts are not copied to runtime stage
4. Check that apt caches are cleaned: `rm -rf /var/lib/apt/lists/*`

---

### Issue 5: Platform Architecture Mismatch

**Cause**: Image built for wrong architecture (e.g., ARM instead of AMD64).

**Solution**:
- Always build with `--platform linux/amd64` flag:
  ```bash
  docker build --platform linux/amd64 -f Dockerfile.vercel -t doc-uni-proc:vercel .
  ```
- Vercel requires `linux/amd64` architecture for container deployments

---

## Quick Reference Commands

### Local Testing

```bash
# Build for Vercel platform
docker build --platform linux/amd64 -f Dockerfile.vercel -t doc-uni-proc:vercel .

# Run with environment variables
docker run -p 8080:8080 --env-file .env.map --env-file .env.secret doc-uni-proc:vercel

# Test health endpoint
curl http://localhost:8080/health

# Stop and remove container
docker stop $(docker ps -q --filter ancestor=doc-uni-proc:vercel)
```

---

### Vercel Deployment

```bash
# Deploy to production
vercel deploy --prod

# Deploy to preview
vercel deploy

# View logs
vercel logs --prod --follow

# List deployments
vercel ls

# Rollback (promote previous deployment)
vercel promote <deployment-url>
```

---

### Debugging

```bash
# Check environment variables
vercel env ls

# View recent error logs
vercel logs --prod --filter=error

# Test production health check
curl -v https://doc-uni-proc.vercel.app/health

# Check deployment status
vercel inspect <deployment-url>
```

---

## Final Checklist

Before marking deployment as complete, verify:

- [ ] Local Docker build succeeds with `--platform linux/amd64`
- [ ] Local container runs successfully with health check returning 200
- [ ] All required environment variables added to Vercel Dashboard
- [ ] Production deployment shows "Ready" status in Vercel
- [ ] Production health check returns 200: `https://doc-uni-proc.vercel.app/health`
- [ ] API endpoints respond correctly (not 401 or 500 errors)
- [ ] No critical errors in Vercel runtime logs
- [ ] Performance baseline established (response time < 1s for health check)
- [ ] Rollback plan documented and tested

---

## Support and Documentation

- **Environment Variables**: See `VERCEL_ENV_SETUP.md` for detailed configuration
- **Dockerfile Reference**: See `Dockerfile.vercel` for build configuration
- **Vercel Documentation**: https://vercel.com/docs/concepts/deployments/containers
- **Docker Best Practices**: https://docs.docker.com/develop/dev-best-practices/

---

**Last Updated**: January 2026  
**Maintained By**: DevOps Team
