# Netlify + Supabase (free plan)

This setup provides:
- Public read-only comments (approved only)
- Auth-required comment + message writes
- Public stats tracking via Netlify functions

## 1) Supabase project
1. Create a Supabase project
2. Open SQL editor and run: `supabase/schema.sql`
3. Enable email auth (Auth > Providers)

## 2) Netlify env vars
Set these in Netlify site settings:
- `SUPABASE_URL`
- `SUPABASE_SERVICE_ROLE_KEY`
- `ADMIN_ALLOWLIST` (comma-separated emails allowed to access admin endpoints)

## 3) API endpoints (Netlify Functions)
- `GET /api/comments-list?page=/docs`
- `POST /api/comments-create` (requires Authorization: Bearer <supabase_jwt>)
- `POST /api/messages-create` (requires Authorization: Bearer <supabase_jwt>)
- `POST /api/stats-track` (public)

Rate limits:
- Comments/messages are limited to 5 requests per minute per IP.

Payload examples:
```json
{ "page": "/docs", "author": "Ada", "rating": 5, "message": "Nice guide" }
```
```json
{ "page": "/docs", "email": "ada@host", "message": "Question" }
```
```json
{ "page": "/docs" }
```

## 4) Auth flow (Supabase)
Use Supabase Auth to obtain a JWT token on the client.
Pass it as:
`Authorization: Bearer <token>`

## 5) Build command
Netlify build command:
`npm run build:site`

Publish directory:
`docs/site/browser`
