import { CommonModule } from '@angular/common';
import { Component } from '@angular/core';

import { AppState } from './app.state';

@Component({
  selector: 'app-status',
  imports: [CommonModule],
  templateUrl: './status.component.html'
})
export class StatusComponent {
  constructor(public state: AppState) {}

  get t() {
    return this.state.t();
  }
}
