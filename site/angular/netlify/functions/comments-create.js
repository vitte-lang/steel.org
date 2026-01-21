const { createClient } = require('@supabase/supabase-js');

const headers = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'content-type, authorization',
  'Access-Control-Allow-Methods': 'POST, OPTIONS'
};

function blockedByHoneypot(payload) {
  const hp = payload?.meta?.hp || '';
  if (hp.trim() !== '') {
    return true;
  }
  const ts = payload?.meta?.ts || 0;
  if (ts && Date.now() - ts < 3000) {
    return true;
  }
  return false;
}

function getClientIp(event) {
  return (
    event.headers['x-nf-client-connection-ip'] ||
    event.headers['x-forwarded-for'] ||
    ''
  )
    .split(',')[0]
    .trim();
}

async function rateLimit(supabase, ip, action, limit) {
  if (!ip) {
    return false;
  }
  const bucket = Math.floor(Date.now() / 60000);
  const { data, error } = await supabase
    .from('rate_limits')
    .select('count')
    .eq('ip', ip)
    .eq('action', action)
    .eq('bucket', bucket)
    .maybeSingle();
  if (error) {
    return false;
  }
  const current = data?.count || 0;
  if (current >= limit) {
    return true;
  }
  await supabase.from('rate_limits').upsert({ ip, action, bucket, count: current + 1 });
  return false;
}

exports.handler = async (event) => {
  if (event.httpMethod === 'OPTIONS') {
    return { statusCode: 204, headers };
  }
  if (event.httpMethod !== 'POST') {
    return { statusCode: 405, headers, body: 'Method Not Allowed' };
  }

  const supabaseUrl = process.env.SUPABASE_URL;
  const serviceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!supabaseUrl || !serviceKey) {
    return { statusCode: 500, headers, body: 'Missing Supabase env vars' };
  }

  const authHeader = event.headers.authorization || '';
  const token = authHeader.startsWith('Bearer ') ? authHeader.slice(7) : '';
  if (!token) {
    return { statusCode: 401, headers, body: 'Missing auth token' };
  }

  const supabase = createClient(supabaseUrl, serviceKey, { auth: { persistSession: false } });
  const ip = getClientIp(event);
  const limited = await rateLimit(supabase, ip, 'comments', 5);
  if (limited) {
    return { statusCode: 429, headers, body: 'Rate limit exceeded' };
  }
  const { data: userData, error: userError } = await supabase.auth.getUser(token);
  if (userError || !userData?.user) {
    return { statusCode: 401, headers, body: 'Invalid auth token' };
  }

  let payload;
  try {
    payload = JSON.parse(event.body || '{}');
  } catch (err) {
    return { statusCode: 400, headers, body: 'Invalid JSON' };
  }

  if (blockedByHoneypot(payload)) {
    return { statusCode: 400, headers, body: 'Blocked' };
  }

  const record = {
    page: payload.page || 'general',
    author: payload.author || userData.user.email || 'user',
    email: payload.email || userData.user.email || null,
    rating: payload.rating || null,
    message: payload.message || ''
  };

  if (!record.message) {
    return { statusCode: 400, headers, body: 'Message is required' };
  }

  const { error } = await supabase.from('comments').insert(record);
  if (error) {
    return { statusCode: 500, headers, body: error.message };
  }

  return { statusCode: 200, headers, body: JSON.stringify({ ok: true }) };
};
