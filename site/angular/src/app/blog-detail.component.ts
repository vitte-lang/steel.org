import { CommonModule } from '@angular/common';
import { Component, OnInit } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute } from '@angular/router';

import { ApiService } from './services/api.service';
import { AppState } from './app.state';
import { SeoService } from './seo.service';
import { SupabaseService } from './services/supabase.service';

@Component({
  selector: 'app-blog-detail',
  imports: [CommonModule, FormsModule],
  templateUrl: './blog-detail.component.html'
})
export class BlogDetailComponent implements OnInit {
  post: any;
  comments: any[] = [];
  status = '';
  authStatus = '';
  authEmail = '';
  authPassword = '';
  form = {
    author: '',
    rating: 5,
    message: '',
    website: '',
    ts: Date.now()
  };
  email: string | null = null;

  constructor(
    private route: ActivatedRoute,
    private state: AppState,
    private api: ApiService,
    private auth: SupabaseService,
    private seo: SeoService
  ) {}

  get t() {
    return this.state.t();
  }

  async ngOnInit(): Promise<void> {
    const slug = this.route.snapshot.paramMap.get('slug');
    this.post = this.t.blog.posts.find((p: any) => p.slug === slug);
    this.updateSeo();
    await this.refreshAuth();
    if (this.post) {
      await this.loadComments(this.post.slug);
      await this.api.trackPage(`/blog/${this.post.slug}`);
    }
  }

  private updateSeo(): void {
    const fallback = this.t.blog;
    if (!this.post) {
      this.seo.update({
        title: this.seo.titleWithSite(fallback.title),
        description: fallback.lead,
        ogType: 'website'
      });
      this.seo.setJsonLd('app-jsonld', null);
      this.seo.setJsonLd('blog-jsonld', null);
      return;
    }

    const url = this.seo.currentUrl();
    this.seo.update({
      title: this.seo.titleWithSite(this.post.title),
      description: this.post.text,
      ogType: 'article',
      url
    });
    this.seo.setJsonLd('app-jsonld', null);
    this.seo.setJsonLd('blog-jsonld', {
      '@context': 'https://schema.org',
      '@type': 'BlogPosting',
      headline: this.post.title,
      description: this.post.text,
      datePublished: this.post.date,
      dateModified: this.post.date,
      author: {
        '@type': 'Organization',
        name: 'Steel - Command'
      },
      mainEntityOfPage: {
        '@type': 'WebPage',
        '@id': url
      },
      url
    });
  }

  async refreshAuth(): Promise<void> {
    this.email = await this.auth.getUserEmail();
    this.form.author = this.email || '';
  }

  async signIn(): Promise<void> {
    this.authStatus = '';
    const err = await this.auth.signIn(this.authEmail, this.authPassword);
    if (err) {
      this.authStatus = err;
      return;
    }
    await this.refreshAuth();
    this.authStatus = 'Signed in.';
  }

  async signOut(): Promise<void> {
    await this.auth.signOut();
    this.email = null;
    this.authStatus = 'Signed out.';
  }

  async signInWithProvider(provider: 'github' | 'google' | 'azure'): Promise<void> {
    this.authStatus = '';
    const err = await this.auth.signInWithProvider(provider);
    if (err) {
      this.authStatus = err;
      return;
    }
    this.authStatus = 'Redirecting to provider...';
  }

  async loadComments(page: string): Promise<void> {
    try {
      this.comments = await this.api.getComments(page);
    } catch (err: any) {
      this.status = err.message || 'Failed to load comments';
    }
  }

  async submitComment(): Promise<void> {
    if (!this.post) {
      return;
    }
    this.status = '';
    try {
      await this.api.postComment({
        page: this.post.slug,
        author: this.form.author || 'user',
        rating: this.form.rating || null,
        message: this.form.message,
        email: this.email || undefined,
        meta: { hp: this.form.website, ts: this.form.ts }
      });
      this.form.message = '';
      this.form.ts = Date.now();
      await this.loadComments(this.post.slug);
      this.status = 'Comment submitted for approval.';
    } catch (err: any) {
      this.status = err.message || 'Failed to submit comment';
    }
  }
}
