import { Component, Input, OnInit } from '@angular/core'
import { ModalController } from '@ionic/angular'
import { BehaviorSubject } from 'rxjs'
import { DiskPartition, ServerModel } from 'src/app/models/server-model'

@Component({
  selector: 'app-backup-confirmation',
  templateUrl: './app-backup-confirmation.component.html',
  styleUrls: ['./app-backup-confirmation.component.scss'],
})
export class AppBackupConfirmationComponent implements OnInit {
  unmasked = false
  password: string
  $error$: BehaviorSubject<string> = new BehaviorSubject('')
  eject = true
  message: string

  @Input() partition: DiskPartition

  constructor (private readonly modalCtrl: ModalController, private readonly serverModel: ServerModel) { }
  ngOnInit () {
    this.message = `Enter your master password to create an encrypted backup of ${this.serverModel.peek().name} to "${this.partition.label || this.partition.logicalname}".`
  }

  toggleMask () {
    this.unmasked = !this.unmasked
  }

  cancel () {
    this.modalCtrl.dismiss({ cancel: true })
  }

  submit () {
    if (!this.password || this.password.length < 12) {
      this.$error$.next('Password must be at least 12 characters in length.')
      return
    }
    const { password, eject } = this
    this.modalCtrl.dismiss({ password, eject })
  }
}
