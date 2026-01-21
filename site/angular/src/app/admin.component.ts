import { CommonModule } from '@angular/common';
import { Component, OnInit } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ApiService } from './services/api.service';
import { SupabaseService } from './services/supabase.service';

@Component({
  selector: 'app-admin',
  imports: [CommonModule, FormsModule],
  templateUrl: './admin.component.html'
})
export class AdminComponent implements OnInit {
  email = '';
  password = '';
  status = '';
  stats: any[] = [];
  comments: any[] = [];
  hasAccess = false;

  constructor(private auth: SupabaseService, private api: ApiService) {}

  async ngOnInit(): Promise<void> {
    await this.refresh();
  }

  async login(): Promise<void> {
    this.status = '';
    const error = await this.auth.signIn(this.email, this.password);
    if (error) {
      this.status = error;
      return;
    }
    await this.refresh();
  }

  async resetPassword(): Promise<void> {
    this.status = '';
    if (!this.email) {
      this.status = 'Enter your email first.';
      return;
    }
    const error = await this.auth.resetPassword(this.email);
    this.status = error ? error : 'Password reset email sent.';
  }

  async logout(): Promise<void> {
    await this.auth.signOut();
    this.stats = [];
    this.comments = [];
    this.hasAccess = false;
  }

  async signInWithProvider(provider: 'github' | 'google' | 'azure'): Promise<void> {
    this.status = '';
    const err = await this.auth.signInWithProvider(provider);
    if (err) {
      this.status = err;
      return;
    }
    this.status = 'Redirecting to provider...';
  }

  async refresh(): Promise<void> {
    try {
      this.stats = await this.api.getStatsAdmin();
      this.comments = await this.api.getCommentsAdmin();
      this.hasAccess = true;
    } catch (err: any) {
      this.status = err.message || 'Login required';
      this.hasAccess = false;
    }
  }

  async approve(id: string): Promise<void> {
    this.status = '';
    try {
      await this.api.approveComment(id);
      await this.refresh();
    } catch (err: any) {
      this.status = err.message || 'Approval failed';
    }
  }
}
