import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const source = await readFile(new URL('./sst.config.ts', import.meta.url), 'utf8')
const localDexSource = await readFile(new URL('../scripts/local-dex-env.mjs', import.meta.url), 'utf8')

test('stores Stripe credentials in stage-scoped SST secrets', () => {
  assert.match(source, /const stripeSecretKey = new sst\.Secret\('STRIPE_SECRET_KEY', ''\)/)
  assert.match(source, /const stripeWebhookSecret = new sst\.Secret\('STRIPE_WEBHOOK_SECRET', ''\)/)
  assert.match(source, /const stripePreviousWebhookSecret = new sst\.Secret\('STRIPE_WEBHOOK_SECRET_PREVIOUS', ''\)/)
})

test('injects Stripe credentials into the API service from SST secret values', () => {
  assert.match(source, /STRIPE_SECRET_KEY: stripeSecretKey\.value/)
  assert.match(source, /STRIPE_WEBHOOK_SECRET: stripeWebhookSecret\.value/)
  assert.match(source, /STRIPE_WEBHOOK_SECRET_PREVIOUS: stripePreviousWebhookSecret\.value/)
})

test('injects billing enforcement rollout settings into the API service', () => {
  assert.match(source, /BILLING_ENFORCEMENT_ENABLED: envOr\('BILLING_ENFORCEMENT_ENABLED', 'false'\)/)
  assert.match(
    source,
    /BILLING_ENFORCEMENT_RISK_WINDOW_SECONDS: envOr\('BILLING_ENFORCEMENT_RISK_WINDOW_SECONDS', '120'\)/,
  )
})

test('injects the cron pause switch used by the usage archive rollout', () => {
  assert.match(source, /DISABLE_CRON_JOBS: envOr\('DISABLE_CRON_JOBS', 'false'\)/)
})

test('preserves an explicit cron pause switch in the local WSL platform', () => {
  assert.match(localDexSource, /DISABLE_CRON_JOBS: process\.env\.DISABLE_CRON_JOBS \?\? 'false'/)
})
