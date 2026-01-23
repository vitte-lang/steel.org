import { CommonModule } from '@angular/common';
import { Component } from '@angular/core';

import { AppState } from './app.state';

@Component({
  selector: 'app-showcase',
  imports: [CommonModule],
  templateUrl: './showcase.component.html'
})
export class ShowcaseComponent {
  constructor(public state: AppState) {}

  get t() {
    return this.state.t();
  }
}
