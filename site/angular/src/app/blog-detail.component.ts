import { CommonModule } from '@angular/common';
import { Component, OnInit } from '@angular/core';
import { ActivatedRoute } from '@angular/router';

import { AppState } from './app.state';
import { SeoService } from './seo.service';

@Component({
  selector: 'app-blog-detail',
  imports: [CommonModule],
  templateUrl: './blog-detail.component.html'
})
export class BlogDetailComponent implements OnInit {
  post: any;

  constructor(private route: ActivatedRoute, private state: AppState, private seo: SeoService) {}

  get t() {
    return this.state.t();
  }

  async ngOnInit(): Promise<void> {
    const slug = this.route.snapshot.paramMap.get('slug');
    this.post = this.t.blog.posts.find((p: any) => p.slug === slug);
    this.updateSeo();
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
}
