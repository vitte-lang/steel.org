import { CommonModule } from '@angular/common';
import { Component } from '@angular/core';

import { AppState } from './app.state';

@Component({
  selector: 'app-security',
  imports: [CommonModule],
  templateUrl: './security.component.html'
})
export class SecurityComponent {
  constructor(public state: AppState) {}

  get t() {
    return this.state.t();
  }
}
