import { CommonModule } from '@angular/common';
import { Component, effect } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AppState } from './app.state';
import { SeoService } from './seo.service';

@Component({
  selector: 'app-blog',
  imports: [CommonModule, RouterLink],
  templateUrl: './blog.component.html'
})
export class BlogComponent {
  constructor(
    public state: AppState,
    private seo: SeoService
  ) {
    effect(() => {
      const t = this.state.t();
      const title = this.seo.titleWithSite(t.blog.title);
      const description = t.blog.lead;
      this.seo.update({ title, description, ogType: 'website' });
      this.seo.setJsonLd('app-jsonld', null);
      this.seo.setJsonLd('blog-jsonld', null);
    });
  }

  get t() {
    return this.state.t();
  }
}
