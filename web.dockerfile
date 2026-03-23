# syntax=docker/dockerfile:1
# =============================================================================
# Stage 1: Frontend builder
# =============================================================================
FROM node:22-bookworm-slim AS frontend-builder
WORKDIR /app

# Install dependencies (cached layer)
COPY package.json package-lock.json ./
RUN npm ci

# Copy frontend source
COPY index.html tsconfig.json tsconfig.node.json vite.config.ts ./
COPY src/ src/
COPY public/ public/

# Build web target
RUN VITE_BUILD_TARGET=web npm run build:web

# =============================================================================
# Runtime — static file server
# =============================================================================
FROM nginx:1.28-bookworm

COPY docker/nginx-web.conf /etc/nginx/conf.d/default.conf
COPY --from=frontend-builder /app/dist-web/ /usr/share/nginx/html/

EXPOSE 80
