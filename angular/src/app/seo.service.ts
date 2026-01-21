import { DOCUMENT } from '@angular/common';
import { Inject, Injectable } from '@angular/core';
import { Meta, Title } from '@angular/platform-browser';

export type SeoMeta = {
  title: string;
  description: string;
  url?: string;
  ogType?: 'website' | 'article';
  twitterCard?: 'summary' | 'summary_large_image';
};

const SITE_NAME = 'Steel - Command';

@Injectable({ providedIn: 'root' })
export class SeoService {
  private jsonLdNodes = new Map<string, HTMLScriptElement>();

  constructor(
    private title: Title,
    private meta: Meta,
    @Inject(DOCUMENT) private doc: Document
  ) {}

  titleWithSite(pageTitle: string): string {
    if (!pageTitle) {
      return SITE_NAME;
    }
    if (pageTitle.includes(SITE_NAME)) {
      return pageTitle;
    }
    return `${pageTitle} | ${SITE_NAME}`;
  }

  update(data: SeoMeta): void {
    this.title.setTitle(data.title);
    this.updateMeta('name', 'description', data.description);

    const url = data.url || this.currentUrl();
    this.updateMeta('property', 'og:title', data.title);
    this.updateMeta('property', 'og:description', data.description);
    this.updateMeta('property', 'og:type', data.ogType || 'website');
    this.updateMeta('property', 'og:url', url);
    this.updateMeta('property', 'og:site_name', SITE_NAME);

    this.updateMeta('name', 'twitter:card', data.twitterCard || 'summary');
    this.updateMeta('name', 'twitter:title', data.title);
    this.updateMeta('name', 'twitter:description', data.description);
  }

  setJsonLd(id: string, payload: Record<string, unknown> | null): void {
    const head = this.doc.head;
    const existing =
      this.jsonLdNodes.get(id) || (this.doc.getElementById(id) as HTMLScriptElement | null);

    if (!payload) {
      if (existing) {
        existing.remove();
      }
      this.jsonLdNodes.delete(id);
      return;
    }

    const script = existing || this.doc.createElement('script');
    script.type = 'application/ld+json';
    script.id = id;
    script.text = JSON.stringify(payload);
    if (!existing) {
      head.appendChild(script);
    }
    this.jsonLdNodes.set(id, script);
  }

  currentUrl(): string {
    return this.doc.defaultView?.location?.href || '';
  }

  private updateMeta(attr: 'name' | 'property', key: string, content: string): void {
    this.meta.updateTag({ [attr]: key, content });
  }
}
