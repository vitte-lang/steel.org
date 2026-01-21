import { Routes } from '@angular/router';

import { adminGuard } from './guards/admin.guard';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('./home.component').then((m) => m.HomeComponent)
  },
  {
    path: 'downloads',
    loadComponent: () => import('./downloads.component').then((m) => m.DownloadsComponent)
  },
  {
    path: 'docs',
    loadComponent: () => import('./docs.component').then((m) => m.DocsComponent)
  },
  {
    path: 'community',
    loadComponent: () => import('./community.component').then((m) => m.CommunityComponent)
  },
  {
    path: 'blog',
    loadComponent: () => import('./blog.component').then((m) => m.BlogComponent)
  },
  {
    path: 'blog/:slug',
    loadComponent: () => import('./blog-detail.component').then((m) => m.BlogDetailComponent)
  },
  {
    path: 'showcase',
    loadComponent: () => import('./showcase.component').then((m) => m.ShowcaseComponent)
  },
  {
    path: 'security',
    loadComponent: () => import('./security.component').then((m) => m.SecurityComponent)
  },
  {
    path: 'status',
    loadComponent: () => import('./status.component').then((m) => m.StatusComponent)
  },
  {
    path: 'admin',
    loadComponent: () => import('./admin.component').then((m) => m.AdminComponent),
    canActivate: [adminGuard]
  },
  { path: '**', redirectTo: '' }
];
