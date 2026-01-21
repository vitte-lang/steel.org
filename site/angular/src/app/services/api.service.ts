import { Injectable } from '@angular/core';

import { SupabaseService } from './supabase.service';

@Injectable({ providedIn: 'root' })
export class ApiService {
  constructor(private supabase: SupabaseService) {}

  async getComments(page: string) {
    const res = await fetch(`/api/comments-list?page=${encodeURIComponent(page)}`);
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async postComment(payload: {
    page: string;
    author: string;
    rating: number | null;
    message: string;
    email?: string;
    meta?: { hp?: string; ts?: number };
  }) {
    const token = await this.supabase.getAccessToken();
    if (!token) {
      throw new Error('Login required');
    }
    const res = await fetch('/api/comments-create', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`
      },
      body: JSON.stringify(payload)
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async postMessage(payload: { page: string; message: string; email?: string; meta?: { hp?: string; ts?: number } }) {
    const token = await this.supabase.getAccessToken();
    if (!token) {
      throw new Error('Login required');
    }
    const res = await fetch('/api/messages-create', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`
      },
      body: JSON.stringify(payload)
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async trackPage(page: string) {
    const res = await fetch('/api/stats-track', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ page })
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async getStatsAdmin() {
    const token = await this.supabase.getAccessToken();
    if (!token) {
      throw new Error('Login required');
    }
    const res = await fetch('/api/stats-admin', {
      headers: { Authorization: `Bearer ${token}` }
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async getCommentsAdmin() {
    const token = await this.supabase.getAccessToken();
    if (!token) {
      throw new Error('Login required');
    }
    const res = await fetch('/api/comments-admin', {
      headers: { Authorization: `Bearer ${token}` }
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }

  async approveComment(id: string) {
    const token = await this.supabase.getAccessToken();
    if (!token) {
      throw new Error('Login required');
    }
    const res = await fetch('/api/comments-approve', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`
      },
      body: JSON.stringify({ id })
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    return res.json();
  }
}
