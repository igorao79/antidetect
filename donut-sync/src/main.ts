import { NestFactory } from "@nestjs/core";
import type { NestExpressApplication } from "@nestjs/platform-express";
import { AppModule } from "./app.module.js";
import { configureBodyParsing } from "./http-setup.js";

const INSECURE_DEFAULT_TOKENS = new Set([
  "secret-sync-token",
  "CHANGE_ME_generate_a_long_random_secret",
  "CHANGE_ME",
]);

function validateEnv() {
  // Without SYNC_TOKEN or SYNC_JWT_PUBLIC_KEY the server admits its members
  // only, by the personal tokens they get when they join by invite.
  const token = process.env.SYNC_TOKEN;
  // A static SYNC_TOKEN is the only credential on a self-hosted server that is
  // typically exposed on 0.0.0.0, so reject the shipped placeholders and any
  // token short enough to brute-force.
  if (token && (INSECURE_DEFAULT_TOKENS.has(token) || token.length < 24)) {
    console.error(
      "SYNC_TOKEN is a known default or too short. Set a long, random secret, e.g. `openssl rand -hex 32`.",
    );
    process.exit(1);
  }
}

async function bootstrap() {
  validateEnv();

  const app = await NestFactory.create<NestExpressApplication>(AppModule, {
    bodyParser: false,
  });
  configureBodyParsing(app);

  app.enableCors({
    origin: "*",
    methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"],
    allowedHeaders: ["Content-Type", "Authorization"],
  });

  const port = process.env.PORT ?? 3929;
  await app.listen(port);
  console.log(`Owl server running on port ${port}`);
}
void bootstrap();
