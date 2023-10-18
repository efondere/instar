use clap::{Args, Parser, Subcommand};
use flate2::read::GzDecoder;
use path_absolutize::*;
use std::io::{BufRead, ErrorKind, Write};
use tar::Archive;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install(InstallArgs),
    Remove(RemoveArgs),
    List,
    Config(ConfigArgs),
}

#[derive(Args)]
struct InstallArgs {
    file_path: std::path::PathBuf,
}

#[derive(Args)]
struct RemoveArgs {
    package_name: String,
}

#[derive(Args)]
struct ConfigArgs {
    config_name: String,
    config_value: String,
}

struct Config {
    install_dir: std::path::PathBuf,
}

impl Config {
    fn load(path: std::path::PathBuf) -> Config {
        if !path.exists() {
            _ = std::fs::File::create(&path);
        }
        let file = match std::fs::File::open(&path) {
            Err(e) => {
                println!(
                    "Failed to open config file ({}): {}. Using default config instead.",
                    path.display(),
                    e
                );
                return Config::default();
            }
            Ok(f) => f,
        };

        let mut cfg = Config::default();

        for line in std::io::BufReader::new(file).lines() {
            if let Ok(str) = line {
                if str.starts_with("install_dir: ") {
                    cfg.install_dir =
                        std::path::PathBuf::from(str.strip_prefix("install_dir: ").unwrap().trim());
                }
            }
        }

        cfg
    }

    fn save_to(self: &Self, path: std::path::PathBuf) {
        let file = std::fs::File::create(path).expect("Failed to create config file.");
        writeln!(&file, "install_dir: {}", self.install_dir.to_str().unwrap()).unwrap();
    }

    fn save(self: &Self) {
        Self::save_to(self, get_config_dir().join("instar.cfg"))
    }
}

impl Default for Config {
    fn default() -> Config {
        let home_dir = std::env::var("HOME").expect("HOME environment variable not found.");
        Config {
            install_dir: std::path::PathBuf::from(home_dir).join(".local"),
        }
    }
}

fn get_config_dir() -> std::path::PathBuf {
    let home_dir = std::env::var("HOME").expect("HOME environment variable not found.");
    let path = std::path::PathBuf::from(home_dir).join(".config/instar/");

    // TODO: use different dirs whether we are installing locally (per-user)
    // or globally (using elevated permissions)
    // ie. don't store globally installed packages on user-specific config
    if !path.exists() {
        std::fs::create_dir_all(&path).expect("Failed to create config directory.");
    }

    path
}

fn is_dir_empty(path: &std::path::PathBuf) -> bool {
    std::fs::read_dir(path).unwrap().count() == 0
}

fn install_tar(file_path: std::path::PathBuf, config_dir: std::path::PathBuf, config: &Config) {
    // STEP 1: open the archive
    let file = match std::fs::File::open(&file_path) {
        Ok(file) => file,
        Err(e) => {
            match e.kind() {
                // TODO: make these errors clearer and perhaps include file_path
                ErrorKind::PermissionDenied => panic!("Permission denied."),
                _ => panic!("Unhandled io exception : {}", e),
            }
        }
    };

    let mut file_str = file_path.file_name().unwrap().to_str().unwrap().to_owned();
    if !file_str.ends_with(".tar.gz") {
        panic!("input file is not a valid tar.gz archive");
    }
    file_str.truncate(file_str.len() - 7);
    println!("Package will be installed under the name: {}", &file_str);

    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    let packages_dir = config_dir.join("packages");
    if !packages_dir.exists() {
        std::fs::create_dir(&packages_dir).expect(
            "Failed to create packages directory. For safety, your package was not installed.",
        );
    }
    let install_file_path = packages_dir.join(&file_str);
    if install_file_path.exists() {
        panic!("The package has already been installed.");
    }
    let install_info_file = std::fs::File::create(install_file_path)
        .expect("Failed to create package file. For safety, the package will not be installed.");

    for e in archive.entries().expect("failed to get entries") {
        let mut e = e.expect("failed to open entry");
        let mut path: std::path::PathBuf = e.path().expect("failed to get path").into();
        let is_dir = path.is_dir();
        path = match path.strip_prefix(&file_str) {
            Ok(p) => p.to_path_buf(),
            Err(_) => path,
        };

        if !path.starts_with("bin")
            && !path.starts_with("etc")
            && !path.starts_with("include")
            && !path.starts_with("lib")
            && !path.starts_with("share")
        {
            continue;
        }

        let absolute_path = config.install_dir.join(&path);
        let absolute_path = absolute_path.as_path().absolutize().unwrap();

        if !is_dir {
            let _ = writeln!(&install_info_file, "{}", absolute_path.to_str().unwrap());
            e.unpack(&absolute_path).expect(
                format!("Failed to extract the file: {}.", absolute_path.display()).as_str(),
            );
        } else {
            let _ =
                std::fs::create_dir_all(&absolute_path).expect("Failed to create dir. Aborting...");
        }
    }
}

fn install(args: InstallArgs) {
    if args.file_path.exists() {
        let config = Config::load(get_config_dir().join("instar.cfg"));
        print!(
            "Installing {} to {}. Continue? [Y/N]: ",
            args.file_path.display(),
            config.install_dir.display()
        );
        std::io::stdout().flush().ok();

        let mut confirmation = String::new();
        std::io::stdin().read_line(&mut confirmation).unwrap();
        confirmation = confirmation.to_lowercase().trim().to_string();
        if confirmation == "y" || confirmation == "yes" {
            println!("Confirmation received.");
        } else {
            println!("No confirmation received. Aborting...");
            return;
        }

        install_tar(args.file_path, get_config_dir(), &config);
    } else {
        println!("File not found: {}", args.file_path.display());
    }
}

fn remove(args: RemoveArgs) {
    let package_file_path = get_config_dir().join("packages").join(args.package_name);
    if !package_file_path.exists() {
        println!("Package is not installed.");
        return;
    }

    // let mut directories: Vec<std::path::PathBuf> = vec![];
    let config = Config::load(get_config_dir().join("instar.cfg"));

    for line in std::fs::read_to_string(&package_file_path).unwrap().lines() {
        let path = std::path::PathBuf::from(line);

        if path.is_dir() {
            continue;
        }
        std::fs::remove_file(&path).unwrap();

        let mut directory = path.parent().unwrap();

        while is_dir_empty(&config.install_dir.join(&directory)) {
            let dir_name = directory.file_name().unwrap().to_str().unwrap();
            if dir_name == "bin"
                || dir_name == "etc"
                || dir_name == "include"
                || dir_name == "lib"
                || dir_name == "share"
            {
                break;
            }

            std::fs::remove_dir(directory).unwrap();

            if let Some(dir) = directory.parent() {
                directory = dir;
            } else {
                break;
            }
        }
    }
    std::fs::remove_file(package_file_path).unwrap();
}

fn list() {
    let dir_it = match std::fs::read_dir(get_config_dir().join("packages")) {
        Ok(d) => d,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                println!("No packages installed.")
            } else {
                println!("Failed to list packages.");
            }
            return;
        }
    };

    for f in dir_it {
        let f = f.unwrap();
        println!("{}", f.path().file_name().unwrap().to_str().unwrap());
    }
}

fn config(args: ConfigArgs) {
    let mut config = Config::load(get_config_dir().join("instar.cfg"));
    match args.config_name.trim() {
        "install_dir" => config.install_dir = std::path::PathBuf::from(args.config_value.trim()),
        _ => {
            println!("Unknown config: {}", args.config_name);
            return;
        }
    };

    config.save();
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install(args) => install(args),
        Commands::Remove(args) => remove(args),
        Commands::List => list(),
        Commands::Config(args) => config(args),
    }
}
