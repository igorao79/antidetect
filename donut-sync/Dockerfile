FROM node:26-alpine AS builder

WORKDIR /build
COPY donut-sync/package.json donut-sync/tsconfig.json donut-sync/tsconfig.build.json ./
COPY donut-sync/src/ src/
RUN npm install
RUN npm run build
RUN npm prune --omit=dev

FROM node:26-alpine

WORKDIR /app
COPY --from=builder /build/package.json .
COPY --from=builder /build/dist/ dist/
COPY --from=builder /build/node_modules/ node_modules/

ENV NODE_ENV=production
ENV PORT=12342
ENV DATA_DIR=/data
RUN mkdir -p /data && chown node:node /data
VOLUME /data
EXPOSE 12342

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD wget -qO- http://127.0.0.1:12342/health > /dev/null || exit 1

USER node
CMD ["node", "dist/main"]
