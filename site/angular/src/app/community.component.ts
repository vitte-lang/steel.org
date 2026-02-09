import { CommonModule } from '@angular/common';
import { Component, effect } from '@angular/core';
import { AppState } from './app.state';
import { SeoService } from './seo.service';

@Component({
  selector: 'app-community',
  imports: [CommonModule],
  templateUrl: './community.component.html'
})
export class CommunityComponent {
  constructor(public state: AppState, private seo: SeoService) {
    effect(() => {
      const t = this.state.t();
      const title = this.seo.titleWithSite(t.community.title);
      const description = t.community.lead;
      this.seo.update({ title, description, ogType: 'website' });
      this.seo.setJsonLd('app-jsonld', null);
      this.seo.setJsonLd('blog-jsonld', null);
    });
  }

  get t() {
    return this.state.t();
  }
}
