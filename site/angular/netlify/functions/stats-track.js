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

  let payload;
  try {
    payload = JSON.parse(event.body || '{}');
  } catch (err) {
    return { statusCode: 400, headers, body: 'Invalid JSON' };
  }

  const page = payload.page || 'unknown';

  const supabase = createClient(supabaseUrl, serviceKey, { auth: { persistSession: false } });
  const { data: existing, error: fetchError } = await supabase
    .from('stats')
    .select('id, views')
    .eq('page', page)
    .maybeSingle();

  if (fetchError) {
    return { statusCode: 500, headers, body: fetchError.message };
  }

  const views = (existing?.views || 0) + 1;
  const { error } = await supabase.from('stats').upsert({ page, views });
  if (error) {
    return { statusCode: 500, headers, body: error.message };
  }

  return { statusCode: 200, headers, body: JSON.stringify({ page, views }) };
};
