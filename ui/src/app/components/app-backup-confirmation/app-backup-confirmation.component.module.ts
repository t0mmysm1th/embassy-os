import { NgModule } from '@angular/core'
import { CommonModule } from '@angular/common'
import { AppBackupConfirmationComponent } from './app-backup-confirmation.component'
import { IonicModule } from '@ionic/angular'
import { RouterModule } from '@angular/router'
import { SharingModule } from 'src/app/modules/sharing.module'
import { FormsModule } from '@angular/forms';

@NgModule({
  declarations: [
    AppBackupConfirmationComponent,
  ],
  imports: [
    CommonModule,
    IonicModule,
    RouterModule.forChild([]),
    SharingModule,
    FormsModule,
  ],
  exports: [AppBackupConfirmationComponent],
})
export class AppBackupConfirmationComponentModule { }
