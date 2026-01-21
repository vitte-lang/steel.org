import { CommonModule } from '@angular/common';
import { Component, effect } from '@angular/core';

import { AppState } from './app.state';
import { SeoService } from './seo.service';

@Component({
  selector: 'app-downloads',
  imports: [CommonModule],
  templateUrl: './downloads.component.html'
})
export class DownloadsComponent {
  constructor(
    public state: AppState,
    private seo: SeoService
  ) {
    effect(() => {
      const t = this.state.t();
      const title = this.seo.titleWithSite(t.download.title);
      const description = t.download.lead;
      this.seo.update({ title, description, ogType: 'website' });
      this.seo.setJsonLd('blog-jsonld', null);
      this.seo.setJsonLd('app-jsonld', {
        '@context': 'https://schema.org',
        '@type': 'SoftwareApplication',
        name: 'Steel - Command',
        description,
        applicationCategory: 'DeveloperApplication',
        operatingSystem: 'macOS, Windows, Linux',
        url: this.seo.currentUrl(),
        downloadUrl: this.state.downloadUrl
      });
    });
  }

  get t() {
    return this.state.t();
  }
}
