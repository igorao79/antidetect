import type { IncomingMessage } from "node:http";
import type { NestExpressApplication } from "@nestjs/platform-express";

/**
 * The body parsing every Owl server runs with. Object transfers under
 * `/v1/blob/` carry arbitrary bytes, JSON-typed or not (an encrypted profile
 * file is not valid JSON), so the JSON parser never reads them; the blob
 * endpoint takes them as they arrive.
 */
export function configureBodyParsing(app: NestExpressApplication): void {
  // biome-ignore lint/correctness/useHookAtTopLevel: NestJS method, not a React hook
  app.useBodyParser("json", {
    limit: "50mb",
    type: (req: IncomingMessage) =>
      !req.url?.startsWith("/v1/blob/") &&
      /\bjson\b|\+json/i.test(String(req.headers["content-type"] ?? "")),
  });
}
