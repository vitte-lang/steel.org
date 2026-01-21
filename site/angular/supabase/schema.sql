-- Supabase schema for Steel site (comments, messages, stats)

create extension if not exists "pgcrypto";

create table if not exists public.comments (
  id uuid primary key default gen_random_uuid(),
  page text not null,
  author text not null,
  email text,
  rating int,
  message text not null,
  approved boolean not null default false,
  created_at timestamptz not null default now()
);

create table if not exists public.messages (
  id uuid primary key default gen_random_uuid(),
  page text not null,
  email text,
  message text not null,
  created_at timestamptz not null default now()
);

create table if not exists public.stats (
  id uuid primary key default gen_random_uuid(),
  page text not null unique,
  views bigint not null default 0,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index if not exists idx_comments_page on public.comments(page);
create index if not exists idx_messages_page on public.messages(page);

create or replace function public.touch_stats_updated_at()
returns trigger as $$
begin
  new.updated_at = now();
  return new;
end;
$$ language plpgsql;

create trigger trg_stats_updated_at
before update on public.stats
for each row execute procedure public.touch_stats_updated_at();

alter table public.comments enable row level security;
alter table public.messages enable row level security;
alter table public.stats enable row level security;

-- Comments: public read only approved, insert requires authenticated user.
create policy "comments_public_read" on public.comments
  for select
  using (approved = true);

create policy "comments_insert_auth" on public.comments
  for insert
  with check (auth.uid() is not null);

-- Messages: insert requires authenticated user, no public read.
create policy "messages_insert_auth" on public.messages
  for insert
  with check (auth.uid() is not null);

-- Stats: public read, write via service role in Netlify functions.
create policy "stats_read" on public.stats
  for select
  using (true);

-- Optional: store blog comments moderation metadata
alter table public.comments
  add column if not exists approved_at timestamptz;

create table if not exists public.rate_limits (
  ip text not null,
  action text not null,
  bucket bigint not null,
  count int not null default 0,
  created_at timestamptz not null default now(),
  primary key (ip, action, bucket)
);

alter table public.rate_limits enable row level security;

create policy "rate_limits_read" on public.rate_limits
  for select
  using (false);
