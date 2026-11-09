import type { AccountNotice } from "../shared/types/api";
import type { LauncherState, LaunchPhase, LogLevel } from "../shared/types/events";
import type { LauncherSettings } from "../shared/types/settings";
import type { DraftErrorCode, NewInstanceErrorCode } from "./store/instancedraft";
import type { UpdateStateCopy } from "./store/updatestate";

export type LauncherLanguage = LauncherSettings["ui"]["language"];

export interface LauncherCopy {
  ready: string;
  tagline: string;
  worldsTitle: string;
  worldsText: string;
  hosting: string;
  discordTitle: string;
  discordText: string;
  version: string;
  releases: string;
  snapshots: string;
  allVersions: string;
  searchVersion: string;
  play: string;
  stop: string;
  language: string;
  discord: string;
  donate: string;
  logs: string;
  about: string;
  settings: string;
  aboutTitle: string;
  logsTitle: string;
  signInFailed: string;
  releaseLabel: (version: string) => string;
  launcherLinks: string;
  launcherTools: string;
  openSettings: string;
  openLogs: string;
  openAbout: string;
  closeModal: string;
  recentTitle: string;
  recentEmpty: string;
  accountStatusPremium: string;
  accountStatusOffline: string;
  accounts: string;
  noSavedAccounts: string;
  forgetAccount: (username: string) => string;
  signOut: string;
  login: {
    session: string;
    premiumSession: string;
    offlineSession: string;
    microsoftSignIn: string;
    microsoftDisabledTitle: string;
    microsoftDisabledText: string;
    deviceCodePrompt: string;
    finishedSignIn: string;
    microsoftHelp: string;
    offlineTitle: string;
    offlineHelp: string;
    offlineUsername: string;
    playerNamePlaceholder: string;
    continueOffline: string;
    signOut: string;
  };
  settingsPanel: {
    eyebrow: string;
    title: string;
    minecraftDirectory: string;
    language: string;
    width: string;
    height: string;
    fullscreen: string;
  };
  logsPanel: {
    eyebrow: string;
    title: string;
    empty: string;
  };
  instances: {
    hero: {
      lastPlayed: (when: string) => string;
      neverPlayed: string;
      noVersion: string;
      noVersionHint: string;
      play: string;
      stop: string;
      openSettings: string;
      quickSelect: string;
      emptyTitle: string;
      emptyText: string;
      emptyAction: string;
    };
    selector: {
      label: string;
      active: string;
      manage: string;
      empty: string;
    };
    state: {
      playingNow: string;
      running: string;
      blocked: string;
      launching: string;
      installing: string;
      stopping: string;
    };
    settingsModal: {
      title: string;
      save: string;
      saving: string;
      cancel: string;
      tabsLabel: string;
      tabs: { general: string; performance: string; display: string };
      fields: {
        name: string;
        version: string;
        memoryMin: string;
        memoryMax: string;
        memoryHint: string;
        javaPath: string;
        javaPathPlaceholder: string;
        useGlobalWindow: string;
        globalWindowValue: (width: number, height: number, fullscreen: boolean) => string;
        width: string;
        height: string;
        fullscreen: string;
      };
      errors: Record<DraftErrorCode, string>;
    };
    drawer: {
      title: string;
      closeLabel: string;
      newInstance: string;
      empty: string;
      active: string;
      select: string;
      settings: string;
      clone: string;
      remove: string;
      removeBlockedActive: string;
      removeBlockedRunning: string;
      confirm: {
        title: string;
        message: (name: string) => string;
        confirm: string;
        removing: string;
        cancel: string;
      };
      form: {
        createTitle: string;
        cloneTitle: string;
        name: string;
        namePlaceholder: string;
        cloneSuffix: string;
        version: string;
        loader: string;
        comingSoon: string;
        copyWorlds: string;
        copyWorldsHint: string;
        create: string;
        clone: string;
        working: string;
        back: string;
        errors: Record<NewInstanceErrorCode, string>;
      };
    };
  };
  aboutPanel: {
    eyebrow: string;
    version: string;
    author: string;
    organization: string;
    website: string;
    license: string;
  };
  updates: {
    versionLabel: (version: string) => string;
    state: UpdateStateCopy;
    indicator: {
      ready: string;
      available: (version: string) => string;
      downloading: (percent: number) => string;
      downloaded: (version: string) => string;
      error: string;
    };
    modal: {
      title: string;
      installedVersion: string;
      newVersion: string;
      releaseDate: string;
      notes: string;
      notesEmpty: string;
      progressLabel: string;
      download: string;
      install: string;
      retry: string;
      blockedRunning: string;
    };
    about: {
      check: string;
    };
  };
  shell: {
    navigation: string;
    language: string;
    play: string;
    instances: string;
    console: string;
    minimize: string;
    maximize: string;
    restore: string;
    close: string;
    noInstance: string;
    community: string;
    donate: string;
  };
  telemetry: {
    title: string;
    phases: Record<LaunchPhase, string>;
    messages: Record<LaunchPhase, string>;
    speed: string;
    files: string;
    active: string;
    verified: string;
    reused: string;
    retries: string;
    of: (done: string, total: string) => string;
    remaining: (time: string) => string;
  };
  console: {
    eyebrow: string;
    title: string;
    search: string;
    levels: Record<LogLevel | "all", string>;
    sources: { all: string; launcher: string; game: string };
    follow: string;
    copy: string;
    copied: string;
    export: string;
    clear: string;
    openFolder: string;
    empty: string;
    noMatches: string;
    count: (shown: number, total: number) => string;
  };
  retry: string;
  startupFailedTitle: string;
  startupFailedMessage: string;
  notices: Record<AccountNotice | "migratedDefaultInstance" | "onedriveDetected", string> & { dismiss: string };
  errors: Record<string, string>;
  status: {
    label: Record<LauncherState, string>;
    progressLabel: string;
    messages: Record<string, string>;
    preparingMinecraft: (versionId: string) => string;
  };
}

export const launcherCopy: Record<LauncherLanguage, LauncherCopy> = {
  es: {
    ready: "Listo para jugar",
    tagline: "Elige versión, ajusta tus mods y a picar diamantes sin launchers pesados ni bloatware.",
    worldsTitle: "JiferCloud, hosting de JiferCraft",
    worldsText: "Crea tu servidor con el hosting oficial de JiferCraft Studios.",
    hosting: "Hosting",
    discordTitle: "Únete al Discord de JiferCraft",
    discordText: "Recibe noticias, soporte y novedades de la comunidad.",
    version: "Versión",
    releases: "Estables",
    snapshots: "Snapshots",
    allVersions: "Todas",
    searchVersion: "Buscar versión",
    play: "Jugar",
    stop: "Detener",
    language: "EN",
    discord: "Discord",
    donate: "Donar",
    logs: "Registros",
    about: "?",
    settings: "Ajustes",
    aboutTitle: "Información",
    logsTitle: "Registros de depuración",
    signInFailed: "Error al iniciar sesión",
    releaseLabel: (version) => `versión/${version}`,
    launcherLinks: "Enlaces del launcher",
    launcherTools: "Herramientas del launcher",
    openSettings: "Abrir ajustes",
    openLogs: "Abrir registros de depuración",
    openAbout: "Abrir información",
    closeModal: "Cerrar ventana",
    recentTitle: "Tus últimas conexiones",
    recentEmpty: "Aún no hay versiones recientes.",
    accountStatusPremium: "Premium",
    accountStatusOffline: "Sin conexión",
    accounts: "Cuentas",
    noSavedAccounts: "No hay cuentas sin conexión guardadas.",
    forgetAccount: (username) => `Olvidar ${username}`,
    signOut: "Cerrar sesión",
    login: {
      session: "Sesión",
      premiumSession: "Sesión premium de Microsoft activa.",
      offlineSession: "Sesión sin conexión activa.",
      microsoftSignIn: "Iniciar sesión con Microsoft",
      microsoftDisabledTitle: "Inicio de sesión con Microsoft desactivado",
      microsoftDisabledText: "La autenticación premium se habilitará más adelante.",
      deviceCodePrompt: "Introduce este código en la página de Microsoft:",
      finishedSignIn: "Ya terminé de iniciar sesión",
      microsoftHelp: "Usa tu cuenta de Microsoft para jugar Minecraft Java Edition con un perfil premium.",
      offlineTitle: "Modo no-premium",
      offlineHelp: "Elige un nombre de jugador y el launcher iniciará Minecraft con una sesión offline local.",
      offlineUsername: "Nombre de usuario sin conexión",
      playerNamePlaceholder: "NombreJugador",
      continueOffline: "Continuar sin conexión",
      signOut: "Cerrar sesión"
    },
    settingsPanel: {
      eyebrow: "Ajustes",
      title: "Launcher",
      minecraftDirectory: "Directorio de Minecraft",
      language: "Idioma",
      width: "Ancho",
      height: "Alto",
      fullscreen: "Pantalla completa"
    },
    logsPanel: {
      eyebrow: "Depuración",
      title: "Registros",
      empty: "Aún no hay registros."
    },
    instances: {
      hero: {
        lastPlayed: (when) => `Última partida: ${when}`,
        neverPlayed: "Aún sin jugar",
        noVersion: "Sin versión",
        noVersionHint: "Elige una versión en los ajustes para poder jugar.",
        play: "Jugar",
        stop: "Detener",
        openSettings: "Ajustes de la instancia",
        quickSelect: "Cambiar de instancia",
        emptyTitle: "Aún no tienes instancias",
        emptyText: "Crea una instancia para elegir versión y empezar a jugar.",
        emptyAction: "Gestionar instancias..."
      },
      selector: {
        label: "Instancias recientes",
        active: "Instancia activa",
        manage: "Gestionar instancias...",
        empty: "No hay instancias todavía."
      },
      state: {
        playingNow: "Jugando ahora",
        running: "En ejecución",
        blocked: "Bloqueado por otra instancia",
        launching: "Iniciando",
        installing: "Instalando",
        stopping: "Deteniendo"
      },
      settingsModal: {
        title: "Ajustes de la instancia",
        save: "Guardar cambios",
        saving: "Guardando...",
        cancel: "Cancelar",
        tabsLabel: "Secciones de ajustes",
        tabs: { general: "General", performance: "Rendimiento", display: "Pantalla" },
        fields: {
          name: "Nombre de la instancia",
          version: "Versión de Minecraft",
          memoryMin: "Memoria mínima",
          memoryMax: "Memoria máxima",
          memoryHint: "Usa un formato como 512M o 2G.",
          javaPath: "Ruta de Java",
          javaPathPlaceholder: "Déjalo vacío para usar java",
          useGlobalWindow: "Usar configuración global de resolución",
          globalWindowValue: (width, height, fullscreen) => `Resolución global: ${width} × ${height}${fullscreen ? ", pantalla completa" : ""}`,
          width: "Ancho",
          height: "Alto",
          fullscreen: "Pantalla completa"
        },
        errors: {
          nameRequired: "Escribe un nombre para la instancia.",
          nameTooLong: "El nombre no puede superar los 64 caracteres.",
          memoryMinFormat: "Usa un formato como 512M o 2G.",
          memoryMaxFormat: "Usa un formato como 512M o 2G.",
          memoryOrder: "La memoria máxima debe ser mayor o igual que la mínima.",
          javaPathInvalid: "La ruta de Java no es válida.",
          widthRange: "El ancho debe estar entre 320 y 7680.",
          heightRange: "El alto debe estar entre 240 y 4320."
        }
      },
      drawer: {
        title: "Instancias",
        closeLabel: "Cerrar biblioteca de instancias",
        newInstance: "Nueva instancia",
        empty: "Aún no tienes instancias. Crea la primera.",
        active: "Activa",
        select: "Seleccionar",
        settings: "Ajustes",
        clone: "Clonar",
        remove: "Eliminar",
        removeBlockedActive: "No puedes eliminar la instancia activa. Selecciona otra primero.",
        removeBlockedRunning: "No puedes eliminar una instancia en ejecución.",
        confirm: {
          title: "Eliminar instancia",
          message: (name) => `Se borrarán la instancia "${name}" y todos sus mundos, mods y ajustes. Esta acción no se puede deshacer.`,
          confirm: "Eliminar",
          removing: "Eliminando...",
          cancel: "Cancelar"
        },
        form: {
          createTitle: "Nueva instancia",
          cloneTitle: "Clonar instancia",
          name: "Nombre",
          namePlaceholder: "Mi survival",
          cloneSuffix: " (copia)",
          version: "Versión de Minecraft",
          loader: "Loader",
          comingSoon: "Próximamente",
          copyWorlds: "Copiar mundos",
          copyWorldsHint: "Sin esto solo se copian mods, configuración y opciones.",
          create: "Crear instancia",
          clone: "Clonar instancia",
          working: "Guardando...",
          back: "Volver a la lista",
          errors: {
            nameRequired: "Escribe un nombre para la instancia.",
            nameTooLong: "El nombre no puede superar los 64 caracteres.",
            versionRequired: "Elige una versión de Minecraft."
          }
        }
      }
    },
    aboutPanel: {
      eyebrow: "Información",
      version: "Versión",
      author: "Autor",
      organization: "Organización",
      website: "Sitio web",
      license: "Licencia"
    },
    updates: {
      versionLabel: (version) => `v${version}`,
      state: {
        unchecked: "Aún no se han buscado actualizaciones.",
        upToDate: "Tienes la versión más reciente.",
        checking: "Buscando actualizaciones…",
        available: (version) => `La versión ${version} está disponible.`,
        downloading: (percent) => `Descargando actualización: ${percent} %.`,
        downloaded: (version) => `La versión ${version} está lista para instalarse.`,
        error: "No se pudo completar la actualización."
      },
      indicator: {
        ready: "Lista",
        available: (version) => `Actualización ${version} disponible`,
        downloading: (percent) => `Descargando actualización (${percent} %)`,
        downloaded: (version) => `Actualización ${version} lista para instalar`,
        error: "Error en la actualización"
      },
      modal: {
        title: "Actualización del launcher",
        installedVersion: "Versión instalada",
        newVersion: "Nueva versión",
        releaseDate: "Fecha de publicación",
        notes: "Novedades",
        notesEmpty: "Esta versión no incluye notas.",
        progressLabel: "Progreso de la descarga",
        download: "Descargar",
        install: "Reiniciar e instalar",
        retry: "Reintentar",
        blockedRunning: "Cierra Minecraft para instalar la actualización."
      },
      about: {
        check: "Buscar actualizaciones"
      }
    },
    shell: {
      navigation: "Navegación principal",
      language: "Idioma",
      play: "Jugar",
      instances: "Instancias",
      console: "Consola",
      minimize: "Minimizar",
      maximize: "Maximizar",
      restore: "Restaurar",
      close: "Cerrar",
      noInstance: "Sin instancia",
      community: "Comunidad",
      donate: "Apoyar el proyecto"
    },
    telemetry: {
      title: "Preparando la partida",
      phases: {
        metadata: "Metadatos",
        files: "Archivos",
        natives: "Nativos",
        java: "Java",
        starting: "Inicio"
      },
      messages: {
        metadata: "Leyendo los metadatos de la versión",
        files: "Verificando y descargando los archivos del juego",
        natives: "Preparando las librerías nativas",
        java: "Preparando Java",
        starting: "Iniciando Minecraft"
      },
      speed: "Velocidad",
      files: "Archivos",
      active: "En curso",
      verified: "SHA verificados",
      reused: "Reutilizados",
      retries: "Reintentos",
      of: (done, total) => `${done} de ${total}`,
      remaining: (time) => `Quedan ${time}`
    },
    console: {
      eyebrow: "Diagnóstico",
      title: "Consola en vivo",
      search: "Filtrar mensajes",
      levels: { all: "Todo", debug: "Depuración", info: "Info", warn: "Avisos", error: "Errores" },
      sources: { all: "Todas las fuentes", launcher: "Launcher", game: "Juego" },
      follow: "Seguir",
      copy: "Copiar",
      copied: "Copiado",
      export: "Exportar",
      clear: "Limpiar",
      openFolder: "Abrir carpeta de datos",
      empty: "Silencio total. Dale a Jugar para ver la consola escupir logs y errores de Forge/Fabric.",
      noMatches: "Ningún mensaje coincide con el filtro.",
      count: (shown, total) => `${shown.toLocaleString("es")} de ${total.toLocaleString("es")} líneas`
    },
    retry: "Reintentar",
    startupFailedTitle: "No pudimos iniciar el launcher",
    startupFailedMessage: "No se pudo leer la configuración local o los permisos de la carpeta de datos.",
    notices: {
      legacySessionDiscarded: "Esta versión guarda tu sesión de Microsoft de otra forma. Inicia sesión de nuevo una sola vez.",
      microsoftExpired: "Tu sesión de Microsoft caducó. Vuelve a iniciar sesión para jugar online.",
      microsoftUnavailable: "No se pudo contactar con Microsoft. Puedes jugar offline mientras tanto.",
      migratedDefaultInstance: "Tus mundos y mods se movieron a la instancia «Default».",
      onedriveDetected: "¡Ojo! Tu carpeta de juego está en OneDrive. Windows intentará sincronizar mientras juegas y te va a corromper los mundos. Muévela a una ruta local antes de perderlo todo.",
      dismiss: "Cerrar aviso"
    },
    errors: {
      already_running: "Tranquilo, vaquero: Minecraft ya se está iniciando o ya está abierto.",
      cancelled: "Operación cancelada.",
      checksum_mismatch: "El archivo se descargó corrupto (falló el checksum SHA). Culpa de la conexión o de Mojang; dale a reintentar.",
      size_mismatch: "Un archivo descargado llegó incompleto. Vuelve a intentarlo.",
      network_unreachable: "No hay conexión con el servidor. Revisa tu red.",
      http_status: "El servidor rechazó la descarga. Inténtalo más tarde.",
      instance_active: "Elige otra instancia antes de borrar esta.",
      instance_busy: "La instancia está en uso por una partida u otra operación.",
      instance_running: "Cierra Minecraft antes de borrar esta instancia.",
      instance_not_found: "La instancia ya no existe.",
      invalid_username: "Usa un nombre de 3 a 16 letras, números o guiones bajos.",
      java_unavailable: "No hay un Java compatible disponible para esta versión.",
      launch_failed: "Minecraft no pudo arrancar.",
      loader_unsupported: "Los loaders de mods aún no se pueden lanzar en esta versión.",
      microsoft_auth_declined: "Se canceló el inicio de sesión de Microsoft.",
      microsoft_auth_expired: "El código de Microsoft caducó. Empieza de nuevo.",
      microsoft_session_expired: "Tu sesión de Microsoft caducó. Inicia sesión otra vez.",
      minecraft_profile_missing: "Esta cuenta de Microsoft no tiene Minecraft: Java Edition.",
      missing_instance: "Selecciona una instancia primero.",
      missing_microsoft_client_id: "El inicio de sesión de Microsoft no está configurado en esta compilación.",
      missing_session: "Inicia sesión antes de jugar.",
      missing_version: "La instancia no tiene versión de Minecraft.",
      update_blocked_running: "Cierra Minecraft para instalar la actualización.",
      update_not_ready: "No hay ninguna actualización descargada.",
      version_not_found: "Esa versión de Minecraft no existe.",
      xbox_account_missing: "Esta cuenta aún no tiene perfil de Xbox. Créalo en xbox.com.",
      xbox_child_account: "Las cuentas infantiles deben añadirse a una familia de Microsoft.",
      xbox_region_blocked: "Xbox Live no está disponible en la región de esta cuenta.",
      xbox_verification_required: "La cuenta necesita verificación de edad en xbox.com."
    },
    status: {
      label: {
        idle: "En espera",
        installing: "Instalando",
        launching: "Iniciando",
        running: "En ejecución",
        stopping: "Deteniendo",
        error: "Error"
      },
      progressLabel: "Progreso actual del launcher",
      messages: {
        "Ready.": "Listo.",
        "Install failed.": "La instalación falló."
      },
      preparingMinecraft: (versionId) => `Preparando Minecraft ${versionId}.`
    }
  },
  en: {
    ready: "Ready to play",
    tagline: "Pick a version, tweak your JVM flags, and launch Minecraft without the bloat.",
    worldsTitle: "JiferCloud, hosting by JiferCraft",
    worldsText: "Create your server with the official hosting from JiferCraft Studios.",
    hosting: "Hosting",
    discordTitle: "Join the JiferCraft Discord",
    discordText: "Get community updates, support, and server news.",
    version: "Version",
    releases: "Releases",
    snapshots: "Snapshots",
    allVersions: "All",
    searchVersion: "Search version",
    play: "Play",
    stop: "Stop",
    language: "ES",
    discord: "Discord",
    donate: "Donate",
    logs: "Logs",
    about: "?",
    settings: "Settings",
    aboutTitle: "About",
    logsTitle: "Debug logs",
    signInFailed: "Sign-in failed",
    releaseLabel: (version) => `release/${version}`,
    launcherLinks: "Launcher links",
    launcherTools: "Launcher tools",
    openSettings: "Open settings",
    openLogs: "Open debug logs",
    openAbout: "Open about",
    closeModal: "Close modal",
    recentTitle: "Your latest sessions",
    recentEmpty: "No recent versions yet.",
    accountStatusPremium: "Premium",
    accountStatusOffline: "Offline",
    accounts: "Accounts",
    noSavedAccounts: "No saved offline accounts.",
    forgetAccount: (username) => `Forget ${username}`,
    signOut: "Sign out",
    login: {
      session: "Session",
      premiumSession: "Premium Microsoft session active.",
      offlineSession: "Offline session active.",
      microsoftSignIn: "Sign in with Microsoft",
      microsoftDisabledTitle: "Microsoft sign-in disabled",
      microsoftDisabledText: "Premium authentication will be enabled later.",
      deviceCodePrompt: "Enter this code on the Microsoft page:",
      finishedSignIn: "I finished signing in",
      microsoftHelp: "Use your Microsoft account to play Minecraft Java Edition with a premium profile.",
      offlineTitle: "Non-premium mode",
      offlineHelp: "Choose a player name and the launcher will start Minecraft with a local offline session.",
      offlineUsername: "Offline username",
      playerNamePlaceholder: "PlayerName",
      continueOffline: "Continue offline",
      signOut: "Sign out"
    },
    settingsPanel: {
      eyebrow: "Settings",
      title: "Launcher",
      minecraftDirectory: "Minecraft directory",
      language: "Language",
      width: "Width",
      height: "Height",
      fullscreen: "Fullscreen"
    },
    logsPanel: {
      eyebrow: "Debug",
      title: "Logs",
      empty: "No logs yet."
    },
    instances: {
      hero: {
        lastPlayed: (when) => `Last played: ${when}`,
        neverPlayed: "Never played",
        noVersion: "No version",
        noVersionHint: "Pick a version in settings to be able to play.",
        play: "Play",
        stop: "Stop",
        openSettings: "Instance settings",
        quickSelect: "Switch instance",
        emptyTitle: "You have no instances yet",
        emptyText: "Create an instance to pick a version and start playing.",
        emptyAction: "Manage instances..."
      },
      selector: {
        label: "Recent instances",
        active: "Active instance",
        manage: "Manage instances...",
        empty: "No instances yet."
      },
      state: {
        playingNow: "Playing now",
        running: "Running",
        blocked: "Blocked by another instance",
        launching: "Launching",
        installing: "Installing",
        stopping: "Stopping"
      },
      settingsModal: {
        title: "Instance settings",
        save: "Save changes",
        saving: "Saving...",
        cancel: "Cancel",
        tabsLabel: "Settings sections",
        tabs: { general: "General", performance: "Performance", display: "Display" },
        fields: {
          name: "Instance name",
          version: "Minecraft version",
          memoryMin: "Minimum memory",
          memoryMax: "Maximum memory",
          memoryHint: "Use a format like 512M or 2G.",
          javaPath: "Java path",
          javaPathPlaceholder: "Leave empty to use java",
          useGlobalWindow: "Use the global resolution",
          globalWindowValue: (width, height, fullscreen) => `Global resolution: ${width} × ${height}${fullscreen ? ", fullscreen" : ""}`,
          width: "Width",
          height: "Height",
          fullscreen: "Fullscreen"
        },
        errors: {
          nameRequired: "Enter a name for the instance.",
          nameTooLong: "The name cannot exceed 64 characters.",
          memoryMinFormat: "Use a format like 512M or 2G.",
          memoryMaxFormat: "Use a format like 512M or 2G.",
          memoryOrder: "Maximum memory must be greater than or equal to the minimum.",
          javaPathInvalid: "The Java path is not valid.",
          widthRange: "Width must be between 320 and 7680.",
          heightRange: "Height must be between 240 and 4320."
        }
      },
      drawer: {
        title: "Instances",
        closeLabel: "Close instance library",
        newInstance: "New instance",
        empty: "You have no instances yet. Create the first one.",
        active: "Active",
        select: "Select",
        settings: "Settings",
        clone: "Clone",
        remove: "Delete",
        removeBlockedActive: "You cannot delete the active instance. Select another one first.",
        removeBlockedRunning: "You cannot delete a running instance.",
        confirm: {
          title: "Delete instance",
          message: (name) => `The instance "${name}" and all its worlds, mods and settings will be deleted. This cannot be undone.`,
          confirm: "Delete",
          removing: "Deleting...",
          cancel: "Cancel"
        },
        form: {
          createTitle: "New instance",
          cloneTitle: "Clone instance",
          name: "Name",
          namePlaceholder: "My survival",
          cloneSuffix: " (copy)",
          version: "Minecraft version",
          loader: "Loader",
          comingSoon: "Coming soon",
          copyWorlds: "Copy worlds",
          copyWorldsHint: "Without this only mods, config and options are copied.",
          create: "Create instance",
          clone: "Clone instance",
          working: "Saving...",
          back: "Back to the list",
          errors: {
            nameRequired: "Enter a name for the instance.",
            nameTooLong: "The name cannot exceed 64 characters.",
            versionRequired: "Choose a Minecraft version."
          }
        }
      }
    },
    aboutPanel: {
      eyebrow: "About",
      version: "Version",
      author: "Author",
      organization: "Organization",
      website: "Website",
      license: "License"
    },
    updates: {
      versionLabel: (version) => `v${version}`,
      state: {
        unchecked: "Updates have not been checked yet.",
        upToDate: "You are on the latest version.",
        checking: "Checking for updates…",
        available: (version) => `Version ${version} is available.`,
        downloading: (percent) => `Downloading update: ${percent}%.`,
        downloaded: (version) => `Version ${version} is ready to install.`,
        error: "The update could not be completed."
      },
      indicator: {
        ready: "Ready",
        available: (version) => `Update ${version} available`,
        downloading: (percent) => `Downloading update (${percent}%)`,
        downloaded: (version) => `Update ${version} ready to install`,
        error: "Update failed"
      },
      modal: {
        title: "Launcher update",
        installedVersion: "Installed version",
        newVersion: "New version",
        releaseDate: "Release date",
        notes: "What's new",
        notesEmpty: "This release has no notes.",
        progressLabel: "Download progress",
        download: "Download",
        install: "Restart and install",
        retry: "Retry",
        blockedRunning: "Close Minecraft to install the update."
      },
      about: {
        check: "Check for updates"
      }
    },
    shell: {
      navigation: "Main navigation",
      language: "Language",
      play: "Play",
      instances: "Instances",
      console: "Console",
      minimize: "Minimize",
      maximize: "Maximize",
      restore: "Restore",
      close: "Close",
      noInstance: "No instance",
      community: "Community",
      donate: "Support the project"
    },
    telemetry: {
      title: "Getting the game ready",
      phases: {
        metadata: "Metadata",
        files: "Files",
        natives: "Natives",
        java: "Java",
        starting: "Start"
      },
      messages: {
        metadata: "Reading version metadata",
        files: "Verifying and downloading game files",
        natives: "Preparing native libraries",
        java: "Preparing Java",
        starting: "Starting Minecraft"
      },
      speed: "Speed",
      files: "Files",
      active: "In flight",
      verified: "SHA verified",
      reused: "Reused",
      retries: "Retries",
      of: (done, total) => `${done} of ${total}`,
      remaining: (time) => `${time} left`
    },
    console: {
      eyebrow: "Diagnostics",
      title: "Live console",
      search: "Filter messages",
      levels: { all: "All", debug: "Debug", info: "Info", warn: "Warnings", error: "Errors" },
      sources: { all: "All sources", launcher: "Launcher", game: "Game" },
      follow: "Follow",
      copy: "Copy",
      copied: "Copied",
      export: "Export",
      clear: "Clear",
      openFolder: "Open data folder",
      empty: "All quiet. Hit Play and watch the console spam scroll by.",
      noMatches: "No message matches the filter.",
      count: (shown, total) => `${shown.toLocaleString("en")} of ${total.toLocaleString("en")} lines`
    },
    retry: "Retry",
    startupFailedTitle: "Could not launch JiferCraft Launcher",
    startupFailedMessage: "Could not read local configuration or data directory permissions.",
    notices: {
      legacySessionDiscarded: "This version stores your Microsoft session differently. Please sign in once more.",
      microsoftExpired: "Your Microsoft session expired. Sign in again to play online.",
      microsoftUnavailable: "Microsoft could not be reached. You can play offline meanwhile.",
      migratedDefaultInstance: "Your worlds and mods were moved into the \"Default\" instance.",
      onedriveDetected: "Heads up: Your game folder is inside OneDrive. Cloud sync WILL lock anvil files and corrupt your worlds mid-game. Move it out to a local path ASAP.",
      dismiss: "Dismiss notice"
    },
    errors: {
      already_running: "Hold your horses — Minecraft is already running (or warming up).",
      cancelled: "Operation cancelled.",
      checksum_mismatch: "Corrupted download detected (SHA hash mismatch). Either your Wi-Fi hiccuped or Mojang's CDN dropped a packet. Hit retry.",
      size_mismatch: "A downloaded file arrived incomplete. Try again.",
      network_unreachable: "The server cannot be reached. Check your connection.",
      http_status: "The server refused the download. Try again later.",
      instance_active: "Select another instance before deleting this one.",
      instance_busy: "The instance is in use by a game or another operation.",
      instance_running: "Close Minecraft before deleting this instance.",
      instance_not_found: "The instance no longer exists.",
      invalid_username: "Use 3 to 16 letters, numbers or underscores.",
      java_unavailable: "No compatible Java runtime is available for this version.",
      launch_failed: "Minecraft could not be started.",
      loader_unsupported: "Mod loaders cannot be launched in this version yet.",
      microsoft_auth_declined: "The Microsoft sign-in was cancelled.",
      microsoft_auth_expired: "The Microsoft code expired. Start again.",
      microsoft_session_expired: "Your Microsoft session expired. Sign in again.",
      minecraft_profile_missing: "This Microsoft account does not own Minecraft: Java Edition.",
      missing_instance: "Select an instance first.",
      missing_microsoft_client_id: "Microsoft sign-in is not configured in this build.",
      missing_session: "Sign in before playing.",
      missing_version: "The instance has no Minecraft version.",
      update_blocked_running: "Close Minecraft to install the update.",
      update_not_ready: "There is no downloaded update yet.",
      version_not_found: "That Minecraft version does not exist.",
      xbox_account_missing: "This account has no Xbox profile yet. Create one on xbox.com.",
      xbox_child_account: "Child accounts must be added to a Microsoft family first.",
      xbox_region_blocked: "Xbox Live is not available in this account's region.",
      xbox_verification_required: "This account needs age verification on xbox.com."
    },
    status: {
      label: {
        idle: "Idle",
        installing: "Installing",
        launching: "Launching",
        running: "Running",
        stopping: "Stopping",
        error: "Error"
      },
      progressLabel: "Current launcher progress",
      messages: {
        "Ready.": "Ready.",
        "Install failed.": "Install failed."
      },
      preparingMinecraft: (versionId) => `Preparing Minecraft ${versionId}.`
    }
  }
};

export function translateStatusMessage(message: string, copy: LauncherCopy): string {
  const preparingMatch = /^Preparing Minecraft (.+)\.$/.exec(message);

  if (preparingMatch) {
    return copy.status.preparingMinecraft(preparingMatch[1]);
  }

  return copy.status.messages[message] ?? message;
}
