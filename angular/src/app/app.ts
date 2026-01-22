import { CommonModule } from '@angular/common';
import { Component, HostListener, OnInit } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';

import { AppState } from './app.state';
import { SupabaseService } from './services/supabase.service';

@Component({
  selector: 'app-root',
  imports: [CommonModule, RouterOutlet, RouterLink, RouterLinkActive],
  templateUrl: './app.html',
  styleUrl: './app.css'
})
export class App implements OnInit {
  authMenuOpen = false;

  constructor(
    public state: AppState,
    private auth: SupabaseService
  ) {}

  ngOnInit(): void {
    this.state.initLang();
  }

  async signInWithProvider(provider: 'github' | 'google' | 'azure'): Promise<void> {
    this.authMenuOpen = false;
    await this.auth.signInWithProvider(provider);
  }

  toggleAuthMenu(): void {
    this.authMenuOpen = !this.authMenuOpen;
  }

  @HostListener('document:click', ['$event'])
  onDocumentClick(event: MouseEvent): void {
    if (!this.authMenuOpen) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (!target || target.closest('.nav-actions')) {
      return;
    }
    this.authMenuOpen = false;
  }
}
