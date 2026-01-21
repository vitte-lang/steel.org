const { createClient } = require('@supabase/supabase-js');

const headers = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'content-type, authorization',
  'Access-Control-Allow-Methods': 'POST, OPTIONS'
};

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
  const { data: userData, error: userError } = await supabase.auth.getUser(token);
  if (userError || !userData?.user) {
    return { statusCode: 401, headers, body: 'Invalid auth token' };
  }
  const allowlist = (process.env.ADMIN_ALLOWLIST || '')
    .split(',')
    .map((v) => v.trim().toLowerCase())
    .filter(Boolean);
  const email = (userData.user.email || '').toLowerCase();
  if (!allowlist.includes(email)) {
    return { statusCode: 403, headers, body: 'Forbidden' };
  }

  let payload;
  try {
    payload = JSON.parse(event.body || '{}');
  } catch (err) {
    return { statusCode: 400, headers, body: 'Invalid JSON' };
  }

  if (!payload.id) {
    return { statusCode: 400, headers, body: 'Missing comment id' };
  }

  const { error } = await supabase
    .from('comments')
    .update({ approved: true })
    .eq('id', payload.id);

  if (error) {
    return { statusCode: 500, headers, body: error.message };
  }

  return { statusCode: 200, headers, body: JSON.stringify({ ok: true }) };
};
