# Build Go API
FROM golang:1.20-alpine AS builder

WORKDIR /app

# Copy go mod files
COPY go/go.mod go/go.sum ./
RUN go mod download

# Copy source
COPY go/ ./

# Build
RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags "-s -w" -o /api ./cmd/api

# Runtime
FROM alpine:latest
RUN apk --no-cache add ca-certificates tzdata
WORKDIR /app
COPY --from=builder /api /app/api

# Run as non-root user
RUN addgroup -S app && adduser -S app -G app
USER app

EXPOSE 8080

# Basic healthcheck hitting the health endpoint if available; adjust path if needed
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:8080/health || exit 1

CMD ["/app/api"]
