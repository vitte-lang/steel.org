import { CommonModule } from '@angular/common';
import { Component } from '@angular/core';

import { AppState } from './app.state';

@Component({
  selector: 'app-downloads',
  imports: [CommonModule],
  templateUrl: './downloads.component.html'
})
export class DownloadsComponent {
  constructor(public state: AppState) {}

  get t() {
    return this.state.t();
  }
}
