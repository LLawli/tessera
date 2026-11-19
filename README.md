# Tessera

**[Português](#português)** · **[English](#english)** · **[Perguntas frequentes / FAQ](FAQ.md)**

---

## Português

O Tessera é uma central de tokens para Flatpak. Ele faz o token A3 ou o certificado em nuvem funcionar nos aplicativos em Flatpak, como o PJeOffice Pro e os navegadores, **sem instalar nenhum pacote de fornecedor no sistema**. Foi feito pensando nos sistemas imutáveis (Bluefin, Aurora, Bazzite, Fedora Silverblue e Kinoite), em que instalar o `.deb` ou o `.rpm` do fabricante do token é difícil ou impossível. Funciona também nas distribuições comuns e com os aplicativos instalados por elas.

*Tessera* era, em latim, a pequena peça que identificava quem a portava.

O Tessera já traz o driver **OpenSC**, software livre, para os muitos cartões e tokens que o OpenSC atende (ePass2003 da Feitian no formato livre, isto é, formatado pelo próprio OpenSC; PIV e YubiKey, OpenPGP, CardOS, IDPrime, carteiras de identidade de vários países).

Os drivers dos fabricantes são extensões, que você instala conforme o seu token (veja [Instalação](#3-instalar-o-driver-do-seu-token)):

| Seu token ou certificado | Extensão |
|---|---|
| GD Burti (StarSign) e outros tokens SafeSign, atualmente os mais usados pela advocacia; traz o **Token Administration**, para trocar e desbloquear o PIN | `io.github.pedrohqb.Tessera.Driver.SafeSign` |
| SafeNet eToken 5100/5110 e cartões IDPrime | `io.github.pedrohqb.Tessera.Driver.SafeNet` |
| ePass2003 (Feitian), como as certificadoras entregam; pede duas configurações únicas, mostradas na página **Drivers** ([abaixo](#epass2003-configuração-única)) | `io.github.pedrohqb.Tessera.Driver.ePass2003` |
| Certificado em nuvem do Serpro, com o aplicativo SerproID | `io.github.pedrohqb.Tessera.Driver.SerproID` |
| Certificado em nuvem RemoteID, da Certisign (experimental; driver independente, feito por engenharia reversa, sem vínculo com a Certisign; mantido pelo autor do RemoteID) | `io.github.pedrohqb.Tessera.Driver.RemoteID` |
| Tokens do OpenSC (ePass2003 no formato livre, PIV, YubiKey e outros) | nenhuma: já vem no Tessera |

Os tokens ficam disponíveis para estes aplicativos, em Flatpak ou instalados pela distribuição:

| Aplicativo | Para quê |
|---|---|
| PJeOffice Pro e Assinador SERPRO | assinar no PJe e documentos em geral |
| Firefox, Chromium, Google Chrome, Microsoft Edge e Brave | entrar com certificado em sites como Projudi e eproc; com os [assinadores web](#web-pki-e-websigner), assinar no e-SAJ e nos sites que usam Web PKI ou WebSigner |
| Papers, Okular, JSignPdf e Master PDF Editor | assinar PDF e conferir assinaturas |
| LibreOffice | assinar documentos e PDFs exportados |
| Thunderbird e Evolution | e-mail assinado e cifrado (S/MIME) |

![O Tessera com um token GD Burti (SafeSign) e o certificado em nuvem SerproID](data/screenshots/main.png)

---

### Distribuições

| Distribuição | Situação |
|---|---|
| Fedora, Bluefin, Aurora, Bazzite, Silverblue e Kinoite | funciona |
| Arch Linux, Manjaro, BigLinux, EndeavourOS e CachyOS | funciona |
| openSUSE Tumbleweed | funciona |
| Ubuntu 26.04 ou mais novo | funciona |
| MX Linux 25 | funciona com o p11-kit do repositório de teste do MX (MX Package Installer → MX Test Repo) |
| **Debian 12 e 13, LMDE 6 e 7, Ubuntu 24.04, Linux Mint 22** | **não funciona:** use o [distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br) |

O que decide é a versão do p11-kit do sistema: a assinatura precisa da 0.25.6 ou mais nova. Veja [por quê](FAQ.md#o-pin-é-pedido-mas-a-assinatura-falha-por-quê).

---

### Instalação

#### 1. Preparar o sistema (uma vez)

O sistema precisa do Flatpak, do serviço de cartões (`pcscd`) e do servidor p11-kit do usuário, que é o que deixa os aplicativos verem os tokens do Tessera.

**Fedora, Bluefin, Aurora, Bazzite, Silverblue e Kinoite** (os pacotes já vêm instalados):

```bash
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**Arch Linux, Manjaro, BigLinux, EndeavourOS e CachyOS:**

```bash
sudo pacman -S flatpak pcsclite ccid
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**openSUSE Tumbleweed:**

```bash
sudo zypper in flatpak pcsc-ccid p11-kit-server
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**Ubuntu 26.04 ou mais novo** (o p11-kit dele não traz o `p11-kit-server.socket`, então o servidor entra como serviço do usuário):

```bash
sudo apt install flatpak pcscd libccid p11-kit
sudo systemctl enable --now pcscd.socket
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/p11-kit-server.service <<'EOF'
[Unit]
Description=p11-kit server
Documentation=man:p11-kit(8)

[Service]
ExecStartPre=/bin/mkdir -p -m 700 %t/p11-kit
ExecStart=/usr/bin/p11-kit server -f -n %t/p11-kit/pkcs11 pkcs11:
Restart=on-failure

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now p11-kit-server.service
```

#### 2. Instalar o Tessera

[![Get it on FlatPark](https://flatpark.org/badges/get-it-on-flatpark.svg)](https://flatpark.org/apps/io.github.pedrohqb.Tessera)

O Tessera é distribuído pelo [FlatPark](https://flatpark.org). A base em que ele roda vem do Flathub, por isso os comandos adicionam os dois repositórios, na mesma instalação do Tessera. Só para você:

```bash
flatpak --user remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --user remote-add --if-not-exists flatpark https://dl.flatpark.org/flatpark.flatpakrepo
flatpak --user install flatpark io.github.pedrohqb.Tessera
```

Para todos os usuários do computador (pede a senha de administrador):

```bash
flatpak --system remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --system remote-add --if-not-exists flatpark https://dl.flatpark.org/flatpark.flatpakrepo
flatpak --system install flatpark io.github.pedrohqb.Tessera
```

As atualizações chegam com o `flatpak update`.

#### 3. Instalar o driver do seu token

Instale a extensão do seu token (tabela no início) de um destes jeitos:

- **Pela loja de aplicativos** (GNOME Software, Discover): na página do Tessera, em **Complementos**, ou pela página do driver no [FlatPark](https://flatpark.org/apps/io.github.pedrohqb.Tessera/), no botão de instalar. Os nomes são **SafeSign IC**, **SafeNet Authentication Client**, **ePass2003 (Feitian)**, **SerproID** e **RemoteID**.
- **Pelo terminal,** na mesma instalação do Tessera (`--user` ou `--system`). Por exemplo, para o GD Burti:

```bash
flatpak --user install flatpark io.github.pedrohqb.Tessera.Driver.SafeSign
```

Na dúvida, abra o Tessera com o token conectado: se faltar o driver, a linha do leitor, na página **Início**, diz qual instalar. A página **Drivers** lista em **Disponíveis para instalar** os que ainda não estão instalados (o RemoteID, marcado como **Experimental**), cada um com um botão para a página dele no FlatPark e outro que copia o comando. Depois de instalar, saia do Tessera pelo menu e abra de novo.

##### ePass2003: configuração única

O driver da Feitian fala com o token direto pelo USB, sem passar pelo serviço de cartões do sistema (`pcscd`). Por isso, depois de instalá-lo, a página **Drivers** mostra duas configurações, cada uma com um comando para copiar e rodar uma vez no terminal:

1. **USB:** deixa o Tessera, e só ele, alcançar os dispositivos USB (`flatpak override --user --device=usb io.github.pedrohqb.Tessera`). Depois, reinicie o Tessera pelo menu.
2. **Serviço de cartões:** faz o `pcscd` deixar o ePass2003 para o driver; se os dois falarem com o token ao mesmo tempo, ele trava até ser desconectado. O comando pede a sua senha. O aviso de que o `pcscd.socket` continua ativo é normal. O comando para desfazer aparece ao passar o mouse sobre a linha.

Enquanto faltar alguma, o Tessera não carrega o driver. Feitas as duas, as linhas somem e o driver começa sozinho; a primeira leitura do token leva uns 15 segundos.

**ePass2003 no formato livre.** Um ePass2003 formatado pelo OpenSC (e não pelo programa da Feitian, como os das certificadoras) é atendido pelo driver OpenSC, que já vem no Tessera, e dispensa essas configurações. Mais que isso: a do serviço de cartões o esconde do OpenSC. Por isso, um computador atende um formato de cada vez. Para usar o formato livre, desligue o driver **ePass2003 (Feitian)** na página **Drivers**: enquanto sobrar algo da configuração, aparecem abaixo dele as linhas **Desfazer a configuração**, cada uma com o comando para copiar e rodar no terminal. A primeira leitura de um token pelo OpenSC pode levar uns 20 segundos; as seguintes são rápidas.

#### 4. Instalar os assinadores web (se precisar)

O **Lacuna Web PKI**, o **Softplan WebSigner** (exigido pelo e-SAJ) e o **Certisign WebSigner** (portais de assinatura da Certisign) são plugins de navegador com um programa por trás. Nos navegadores em Flatpak, cada um é uma extensão do Tessera. Instale só os que usar, do mesmo jeito que o driver: pela loja (os nomes são **Lacuna Web PKI**, **Softplan WebSigner** e **Certisign WebSigner**) ou pelo terminal, na mesma instalação do Tessera (`--user` ou `--system`):

```bash
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.WebPKI
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.SoftplanWebSigner
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.CertisignWebSigner
```

Depois, reinicie o Tessera pelo menu e configure cada navegador (veja [Web PKI e WebSigner](#web-pki-e-websigner), em Uso). Nos navegadores nativos, as extensões não são necessárias.

---

### Uso

Abra o **Tessera** pelo menu de aplicativos. A janela tem quatro páginas, na barra lateral: **Início**, **Drivers**, **Aplicativos** e **Sistema**. No Início, uma linha diz se está tudo funcionando ou o que precisa de atenção, e o token conectado aparece em **Tokens**, com os seus certificados. O certificado SerproID aparece depois de você entrar pelo aplicativo SerproID, no botão da linha dele na página **Drivers**, onde também se liga e desliga cada driver.

A página **Aplicativos** tem duas abas, **Flatpak** e **Nativos**. Cada aplicativo mostra o que falta e, quando tudo estiver certo, aparece como **Pronto**. Feche o aplicativo antes de configurá-lo.

#### Aplicativos em Flatpak

- **Passo 1 · acesso ao servidor p11-kit do sistema:** um comando para rodar uma vez no terminal, com botão de copiar (por exemplo, `flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 org.mozilla.firefox`). Depois, abra o aplicativo de novo.
- **Passo 2 · banco de certificados:** o botão **Configurar**.

| Aplicativo | Passos |
|---|---|
| PJeOffice Pro e Assinador SERPRO ([FlatPark](https://flatpark.org)) | nenhum |
| Firefox, Thunderbird, Papers, Okular, LibreOffice, JSignPdf e Master PDF Editor | passo 1 e passo 2 |
| Chromium, Chrome, Edge, Brave e Evolution | só o passo 1 |

#### Aplicativos nativos

São os instalados pela própria distribuição. O único passo é o botão **Configurar**, que acrescenta o p11-kit ao banco de certificados do aplicativo.

| Aplicativo nativo | Onde o Tessera configura |
|---|---|
| Firefox e Thunderbird | todos os perfis |
| Chromium, Chrome, Edge, Brave e Evolution | o banco que eles compartilham |
| Okular e Papers | usam o banco do Firefox: ficam prontos quando o Firefox estiver configurado |
| LibreOffice | um banco próprio, indicado como caminho do certificado |
| PJeOffice Pro | a lista de bibliotecas A3 (`list.a3`) |

No Fedora e derivados, o próprio sistema já faz isso, e os aplicativos nativos aparecem como **Pronto**, sem botão.

#### Web PKI e WebSigner

- **Nos navegadores em Flatpak** (Firefox, Chromium, Chrome, Edge e Brave), com as extensões instaladas ([passo 4](#4-instalar-os-assinadores-web-se-precisar)), a seção **Assinadores web** da janela (página Aplicativos, aba Flatpak) mostra, para cada navegador, um comando para rodar uma vez no terminal (botão de copiar). Depois, reinicie o navegador, instale a extensão do plugin na loja do navegador, se ainda não tiver, e marque uma vez, nas configurações do plugin, **"Dispositivos SafeSign AET"** (Web PKI e WebSigner da Softplan, aba "Cripto Dispositivos") ou **"SafeSign"** (WebSigner da Certisign). O passo 1 do navegador, na mesma página, precisa estar feito. Detalhes em [docs/signers.md](docs/signers.md).
- **Nos navegadores nativos**, não é preciso nada disso: use os pacotes oficiais ([Web PKI](https://get.webpkiplugin.com/) e [WebSigner](https://websigner.softplan.com.br/)). No plugin, em "Cripto Dispositivos" → "Opções personalizadas", acrescente o caminho do p11-kit da sua distribuição. As chaves "Tokens SafeNet" e "Dispositivos SafeSign AET" podem ficar desligadas.

  | Distribuição | Caminho |
  |---|---|
  | Fedora e openSUSE | `/usr/lib64/p11-kit-proxy.so` |
  | Ubuntu | `/usr/lib/x86_64-linux-gnu/p11-kit-proxy.so` |
  | Arch e derivados | `/usr/lib/p11-kit-proxy.so` |

Fechar a janela não desliga o Tessera: ele fica na bandeja. Para dúvidas e problemas, veja as [perguntas frequentes](FAQ.md).

---

### A fazer

- **Testar com token real** os outros cartões do OpenSC (até agora, só o ePass2003 no formato livre, no Firefox e no PJeOffice) e os aplicativos acrescentados por último: Thunderbird, Evolution, LibreOffice e Assinador SERPRO (o JSignPdf e o Master PDF Editor já foram testados).
- **Atualizar o OpenSC embutido** quando sair uma versão com as correções do ePass2003 já aceitas no projeto em 18 de novembro de 2026 ([#3892](https://github.com/OpenSC/OpenSC/pull/3892) e [#3921](https://github.com/OpenSC/OpenSC/pull/3921)), ainda sem versão publicada; com elas, o OpenSC do Tessera também poderá formatar o token e gravar certificados nele. Hoje ele lê e assina.
- **Testar o RemoteID** com uma conta de verdade.
- **Testar a extensão SafeNet** com um token de verdade. O driver já funcionou quando vinha embutido no Tessera; falta confirmar o empacotamento como extensão, agora com a versão 10.9.6885, do pacote da GlobalSign (SafeSign e SerproID já foram testados).
- **Bird ID (certificado em nuvem da Soluti), só se alguém pedir e puder testar.** O pacote Linux da Soluti traz um driver PKCS#11 pequeno, que fala direto com a Soluti pela internet; a extensão levaria só ele, com a configuração criada uma vez pelo aplicativo oficial. Não será feita sem um pedido de quem tenha um certificado Bird ID para testar. Veja [Drivers](docs/drivers.md#bird-id-not-packaged) (em inglês).

---

### Documentação, suporte e licença

- **[Perguntas frequentes](FAQ.md)**, em português e inglês.
- **Documentação técnica**, em inglês, em [docs/](docs/README.md): uso da janela e da linha de comando, aplicativos, assinadores web, drivers, compatibilidade (o que já foi testado, e como relatar o seu teste), solução de problemas, arquitetura, histórico e desenvolvimento.
- **Suporte:** abra uma [issue](https://github.com/pedrohqb/tessera/issues) ou peça ajuda no grupo oficial no [Telegram](https://t.me/advogados_linux).
- **Licença:** GPL-3.0-or-later. Os drivers dos fornecedores não fazem parte do Tessera: são extensões à parte, baixadas dos fornecedores na instalação, e seguem as licenças deles. O OpenSC, que vem compilado no Tessera, é LGPL-2.1-or-later.

---

## English

Tessera is a token hub for Flatpak. It makes A3 tokens and cloud certificates work in Flatpak apps, such as PJeOffice Pro and web browsers, **without installing any vendor package on the system**. It was made with immutable systems in mind (Bluefin, Aurora, Bazzite, Fedora Silverblue and Kinoite), where installing a token vendor's `.deb` or `.rpm` is hard or impossible. It also works on regular distributions and with the apps they install.

*Tessera* was the Latin word for a small token that identified its bearer.

Tessera comes with the **OpenSC** driver, free software, for the many cards and tokens OpenSC supports (Feitian's ePass2003 in the free format, that is, formatted by OpenSC itself; PIV and YubiKey, OpenPGP, CardOS, IDPrime, national ID cards).

The vendors' drivers are extensions, installed for the token you have (see [Installation](#3-install-your-tokens-driver)):

| Your token or certificate | Extension |
|---|---|
| GD Burti (StarSign) and other SafeSign tokens, currently the most used by Brazilian lawyers; comes with **Token Administration**, to change and unlock the PIN | `io.github.pedrohqb.Tessera.Driver.SafeSign` |
| SafeNet eToken 5100/5110 and IDPrime cards | `io.github.pedrohqb.Tessera.Driver.SafeNet` |
| Feitian's ePass2003, as Brazilian certificate authorities deliver it; needs two one-time steps, shown on the **Drivers** page ([below](#epass2003-one-time-setup)) | `io.github.pedrohqb.Tessera.Driver.ePass2003` |
| Serpro's cloud certificate, with the SerproID app | `io.github.pedrohqb.Tessera.Driver.SerproID` |
| Certisign's RemoteID cloud certificate (experimental; independent driver, made by reverse engineering, not affiliated with Certisign; maintained by RemoteID's developer) | `io.github.pedrohqb.Tessera.Driver.RemoteID` |
| OpenSC's tokens (ePass2003 in the free format, PIV, YubiKey and others) | none: built into Tessera |

The tokens are available to these apps, as Flatpaks or installed from the distribution:

| App | For |
|---|---|
| PJeOffice Pro and Assinador SERPRO | signing in PJe and documents in general |
| Firefox, Chromium, Google Chrome, Microsoft Edge and Brave | certificate login on sites such as Projudi and eproc; with the [web signers](#web-pki-and-websigner), sign on e-SAJ and on sites that use Web PKI or WebSigner |
| Papers, Okular, JSignPdf and Master PDF Editor | signing PDFs and checking signatures |
| LibreOffice | signing documents and exported PDFs |
| Thunderbird and Evolution | signed and encrypted e-mail (S/MIME) |

![Tessera with a GD Burti (SafeSign) token and the SerproID cloud certificate](data/screenshots/main.png)

### Distributions

| Distribution | Status |
|---|---|
| Fedora, Bluefin, Aurora, Bazzite, Silverblue and Kinoite | works |
| Arch Linux, Manjaro, BigLinux, EndeavourOS and CachyOS | works |
| openSUSE Tumbleweed | works |
| Ubuntu 26.04 or newer | works |
| MX Linux 25 | works with the p11-kit from MX's test repository (MX Package Installer → MX Test Repo) |
| **Debian 12 and 13, LMDE 6 and 7, Ubuntu 24.04, Linux Mint 22** | **does not work:** use [distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br) |

What decides it is the system's p11-kit version: signing needs 0.25.6 or newer. See [why](FAQ.md#the-pin-is-asked-but-signing-fails-why).

### Installation

#### 1. Prepare the system (once)

The system needs Flatpak, the smart card service (`pcscd`) and the user's p11-kit server, which is what lets apps see Tessera's tokens.

**Fedora, Bluefin, Aurora, Bazzite, Silverblue and Kinoite** (the packages are already installed):

```bash
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**Arch Linux, Manjaro, BigLinux, EndeavourOS and CachyOS:**

```bash
sudo pacman -S flatpak pcsclite ccid
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**openSUSE Tumbleweed:**

```bash
sudo zypper in flatpak pcsc-ccid p11-kit-server
sudo systemctl enable --now pcscd.socket
systemctl --user enable --now p11-kit-server.socket
```

**Ubuntu 26.04 or newer** (its p11-kit ships no `p11-kit-server.socket`, so the server goes in as a user service):

```bash
sudo apt install flatpak pcscd libccid p11-kit
sudo systemctl enable --now pcscd.socket
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/p11-kit-server.service <<'EOF'
[Unit]
Description=p11-kit server
Documentation=man:p11-kit(8)

[Service]
ExecStartPre=/bin/mkdir -p -m 700 %t/p11-kit
ExecStart=/usr/bin/p11-kit server -f -n %t/p11-kit/pkcs11 pkcs11:
Restart=on-failure

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now p11-kit-server.service
```

#### 2. Install Tessera

[![Get it on FlatPark](https://flatpark.org/badges/get-it-on-flatpark.svg)](https://flatpark.org/apps/io.github.pedrohqb.Tessera)

Tessera is distributed through [FlatPark](https://flatpark.org). The base it runs on comes from Flathub, so the commands add both remotes, in the same installation as Tessera. For yourself only:

```bash
flatpak --user remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --user remote-add --if-not-exists flatpark https://dl.flatpark.org/flatpark.flatpakrepo
flatpak --user install flatpark io.github.pedrohqb.Tessera
```

For every user of the computer (asks for the administrator password):

```bash
flatpak --system remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --system remote-add --if-not-exists flatpark https://dl.flatpark.org/flatpark.flatpakrepo
flatpak --system install flatpark io.github.pedrohqb.Tessera
```

Updates come with `flatpak update`.

#### 3. Install your token's driver

Install your token's extension (table at the top) in one of these ways:

- **From the app store** (GNOME Software, Discover): on Tessera's page, under **Add-ons**, or from the driver's page on [FlatPark](https://flatpark.org/apps/io.github.pedrohqb.Tessera/), with its install button. The names are **SafeSign IC**, **SafeNet Authentication Client**, **ePass2003 (Feitian)**, **SerproID** and **RemoteID**.
- **From a terminal,** in the same installation as Tessera (`--user` or `--system`). For a GD Burti, say:

```bash
flatpak --user install flatpark io.github.pedrohqb.Tessera.Driver.SafeSign
```

If unsure, open Tessera with the token plugged in: when its driver is missing, the reader's row, on the **Home** page, says which one to install. The **Drivers** page lists under **Available to install** the ones not installed yet (RemoteID marked **Experimental**), each with a button to its FlatPark page and another that copies the command. After installing, quit Tessera from its menu and open it again.

##### ePass2003: one-time setup

Feitian's driver talks to the token over USB itself, not through the system's smart card service (`pcscd`). So, once it is installed, the **Drivers** page shows two steps, each with a command to copy and run once in a terminal:

1. **USB:** lets Tessera, and only Tessera, reach the USB devices (`flatpak override --user --device=usb io.github.pedrohqb.Tessera`). Then restart Tessera from its menu.
2. **Card service:** makes `pcscd` leave the ePass2003 to the driver; if both talk to the token at once, it hangs until it is unplugged. The command asks for your password. The warning that `pcscd.socket` is still active is expected. The command that undoes it shows when you hover over the row.

Until both are done, Tessera does not load the driver. Once they are, the rows go away and the driver starts by itself; the first read of the token takes about 15 seconds.

**ePass2003 in the free format.** An ePass2003 formatted by OpenSC (not by Feitian's software, as the certificate authorities' are) is served by the OpenSC driver built into Tessera and needs none of this setup. More than that: the card service step hides it from OpenSC. So one computer serves one format at a time. To use the free format, switch the **ePass2003 (Feitian)** driver off on the **Drivers** page: while something of its setup is left, the rows **Undo the setup** show below it, each with a command to copy and run in a terminal. OpenSC's first read of a token can take about 20 seconds; the next ones are quick.

#### 4. Install the web signers (if you need them)

**Lacuna Web PKI**, **Softplan WebSigner** (required by e-SAJ) and **Certisign WebSigner** (Certisign's signing portals) are browser plugins with a program behind them. In Flatpak browsers, each is an extension of Tessera. Install only the ones you use, the same way as the driver: from the app store (the names are **Lacuna Web PKI**, **Softplan WebSigner** and **Certisign WebSigner**) or from a terminal, in the same installation as Tessera (`--user` or `--system`):

```bash
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.WebPKI
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.SoftplanWebSigner
flatpak --system install flatpark io.github.pedrohqb.Tessera.Signer.CertisignWebSigner
```

Then restart Tessera from its menu and set up each browser (see [Web PKI and WebSigner](#web-pki-and-websigner), under Usage). Native browsers do not need the extensions.

### Usage

Open **Tessera** from the app menu. The window has four pages, in the sidebar: **Home**, **Drivers**, **Apps** and **System**. On Home, one line says whether everything works or what needs attention, and the plugged-in token appears under **Tokens**, with its certificates. The SerproID certificate appears after you sign in through the SerproID app, with the button on its row on the **Drivers** page, where each driver is also switched on and off.

The **Apps** page has two tabs, **Flatpak** and **Native**. Each app shows what is missing and, when everything is right, shows as **Ready**. Close an app before setting it up.

#### Flatpak apps

- **Step 1 · access to the system's p11-kit server:** a command to run once in a terminal, with a copy button (for example, `flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 org.mozilla.firefox`). Then open the app again.
- **Step 2 · certificate database:** the **Set up** button.

| App | Steps |
|---|---|
| PJeOffice Pro and Assinador SERPRO ([FlatPark](https://flatpark.org)) | none |
| Firefox, Thunderbird, Papers, Okular, LibreOffice, JSignPdf and Master PDF Editor | step 1 and step 2 |
| Chromium, Chrome, Edge, Brave and Evolution | step 1 only |

#### Native apps

These are the apps installed from the distribution itself. The only step is the **Set up** button, which adds p11-kit to the app's certificate database.

| Native app | Where Tessera sets it up |
|---|---|
| Firefox and Thunderbird | every profile |
| Chromium, Chrome, Edge, Brave and Evolution | the database they share |
| Okular and Papers | they use Firefox's database: ready once Firefox is set up |
| LibreOffice | a database of its own, set as its certificate path |
| PJeOffice Pro | the list of A3 libraries (`list.a3`) |

On Fedora and its derivatives, the system itself already does this, and native apps show as **Ready**, with no button.

#### Web PKI and WebSigner

- **In Flatpak browsers** (Firefox, Chromium, Chrome, Edge and Brave), with the extensions installed ([step 4](#4-install-the-web-signers-if-you-need-them)), the window's **Web signers** section (Apps page, Flatpak tab) shows, for each browser, a command to run once in a terminal (copy button). Then restart the browser, install the plugin's extension from the browser's store if it is not there yet and tick once, in the plugin's settings, **"Dispositivos SafeSign AET"** (Web PKI and Softplan's WebSigner, "Cripto Dispositivos" tab) or **"SafeSign"** (Certisign's WebSigner). The browser's step 1, on the same page, must be done. Details in [docs/signers.md](docs/signers.md).
- **In native browsers**, none of this is needed: use the official packages ([Web PKI](https://get.webpkiplugin.com/) and [WebSigner](https://websigner.softplan.com.br/)). In the plugin, under "Cripto Dispositivos" → "Opções personalizadas", add your distribution's p11-kit path. The "Tokens SafeNet" and "Dispositivos SafeSign AET" switches can stay off.

  | Distribution | Path |
  |---|---|
  | Fedora and openSUSE | `/usr/lib64/p11-kit-proxy.so` |
  | Ubuntu | `/usr/lib/x86_64-linux-gnu/p11-kit-proxy.so` |
  | Arch and derivatives | `/usr/lib/p11-kit-proxy.so` |

Closing the window does not stop Tessera: it stays in the tray. For questions and problems, see the [FAQ](FAQ.md).

### To do

- **Test with a real token** OpenSC's other cards (so far, only the ePass2003 in the free format, in Firefox and PJeOffice) and the apps added last: Thunderbird, Evolution, LibreOffice and Assinador SERPRO (JSignPdf and Master PDF Editor have been tested).
- **Update the built-in OpenSC** when a release carries the ePass2003 fixes merged upstream on November 18, 2026 ([#3892](https://github.com/OpenSC/OpenSC/pull/3892) and [#3921](https://github.com/OpenSC/OpenSC/pull/3921)), not yet in a release; with them, Tessera's OpenSC will also be able to format the token and write certificates to it. Today it reads and signs.
- **Test RemoteID** with a real account.
- **Test the SafeNet extension** with a real token. The driver already worked when it was built into Tessera; what is left is confirming its packaging as an extension, now with version 10.9.6885, from GlobalSign's package (SafeSign and SerproID have been tested).
- **Bird ID (Soluti's cloud certificate), only if someone asks and can test it.** Soluti's Linux package has a small PKCS#11 driver that talks to Soluti over the internet; the extension would take only that, with the settings made once by the official app. It will not be made without a request from someone with a Bird ID certificate to test it. See [Drivers](docs/drivers.md#bird-id-not-packaged).

### Documentation, support and license

- **[FAQ](FAQ.md)**, in Portuguese and English.
- **Technical documentation** in [docs/](docs/README.md): using the window and the command line, apps, web signers, drivers, compatibility (what has been tested, and how to report your test), troubleshooting, architecture, history and development.
- **Support:** open an [issue](https://github.com/pedrohqb/tessera/issues) or ask in the [Telegram](https://t.me/advogados_linux) group (in Portuguese).
- **License:** GPL-3.0-or-later. The vendor drivers are not part of Tessera: they are separate extensions, downloaded from their vendors at installation, and are covered by the vendors' own licenses. OpenSC, built into Tessera, is LGPL-2.1-or-later.
