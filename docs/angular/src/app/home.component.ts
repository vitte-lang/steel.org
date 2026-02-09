import { CommonModule } from '@angular/common';
import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AppState } from './app.state';

@Component({
  selector: 'app-home',
  imports: [CommonModule, RouterLink],
  templateUrl: './home.component.html'
})
export class HomeComponent {
  constructor(public state: AppState) {}

  get t() {
    return this.state.t();
  }

  get examples() {
    return this.state.examples.slice(0, 3);
  }
}
