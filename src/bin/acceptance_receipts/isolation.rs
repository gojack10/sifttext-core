//! Behavioral installed/source isolation checks; moves, restoration and drain stay with the launcher.
use std::{fs, io, path::Path};

fn equal(left: &Path, right: &Path) -> Result<(), String> {
    if fs::read(left).map_err(|e| format!("{}: {e}", left.display()))?
        != fs::read(right).map_err(|e| format!("{}: {e}", right.display()))?
    {
        return Err(format!(
            "Isolation bytes differ: {} / {}",
            left.display(),
            right.display()
        ));
    }
    Ok(())
}

fn absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
        Ok(_) => Err(format!(
            "Isolation path unexpectedly exists: {}",
            path.display()
        )),
    }
}

pub fn run(args: &[String]) -> Result<String, String> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["linux-isolation-before", source_exe, installed_exe] => {
            equal(Path::new(source_exe), Path::new(installed_exe))?;
            Ok("isolation-executables=equal".into())
        }
        ["linux-isolation-hidden", evidence, source, saved_build, marker] => {
            let evidence = Path::new(evidence);
            let source = Path::new(source);
            let marker = Path::new(marker);
            equal(&evidence.join("isolation-before.json"), &evidence.join("isolation-hidden.json"))?;
            absent(&source.join("build"))?;
            absent(&source.join("target"))?;
            let name = marker.file_name().ok_or("Missing isolation marker name")?;
            if fs::read(marker).map_err(|e| e.to_string())? != b"installed-only-custody-control\n" {
                return Err("Installed isolation marker bytes differ".into());
            }
            absent(&Path::new(saved_build).join(name))?;
            Ok("isolation-runtime=equal marker=independent".into())
        }
        _ => Err("Usage: linux-isolation-before SOURCE_EXE INSTALLED_EXE | linux-isolation-hidden EVIDENCE SOURCE SAVED_BUILD MARKER".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolation_checks_bytes_hidden_outputs_and_marker_independence() {
        let root =
            std::env::temp_dir().join(format!("sifttext-isolation-check-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let saved = root.join("saved-build");
        fs::create_dir(&saved).unwrap();
        let a = root.join("isolation-before.json");
        let b = root.join("isolation-hidden.json");
        fs::write(&a, b"checked").unwrap();
        fs::write(&b, b"checked").unwrap();
        let before = vec![
            "linux-isolation-before".into(),
            a.display().to_string(),
            b.display().to_string(),
        ];
        assert!(run(&before).is_ok());
        fs::write(&b, b"wrong").unwrap();
        assert!(run(&before).is_err());
        fs::remove_file(&b).unwrap();
        assert!(run(&before).is_err());
        fs::write(&b, b"checked").unwrap();
        let marker = root.join("marker");
        fs::write(&marker, b"installed-only-custody-control\n").unwrap();
        let hidden = vec![
            "linux-isolation-hidden".into(),
            root.display().to_string(),
            root.display().to_string(),
            saved.display().to_string(),
            marker.display().to_string(),
        ];
        assert!(run(&hidden).is_ok());
        for name in ["build", "target"] {
            let path = root.join(name);
            fs::create_dir(&path).unwrap();
            assert!(run(&hidden).is_err());
            fs::remove_dir(&path).unwrap();
        }
        fs::write(saved.join("marker"), b"alias").unwrap();
        assert!(run(&hidden).is_err());
        fs::remove_file(saved.join("marker")).unwrap();
        fs::write(&marker, b"wrong").unwrap();
        assert!(run(&hidden).is_err());
        fs::remove_file(&marker).unwrap();
        assert!(run(&hidden).is_err());
        fs::write(&marker, b"installed-only-custody-control\n").unwrap();
        fs::write(&b, b"wrong").unwrap();
        assert!(run(&hidden).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("missing"), root.join("target")).unwrap();
            assert!(absent(&root.join("target")).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
