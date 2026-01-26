use image::{imageops, DynamicImage, Rgba};
use std::path::{Path, PathBuf};
use anyhow::Result;
use imageproc::drawing::{draw_text_mut, text_size};
use rusttype::{Font, Scale};
use std::fs;
use std::env;

pub struct ImageProcessor;

impl ImageProcessor {
    /// Retourne le chemin de base des overlays
    fn get_overlays_base_path() -> PathBuf {
        // 1. Variable d'environnement (priorité)
        if let Ok(path) = env::var("OVERLAYS_PATH") {
            let p = PathBuf::from(path);
            if p.exists() {
                return p;
            }
        }
        
        // 2. ~/.config/rustizarr/overlays
        if let Some(config_dir) = dirs::config_dir() {
            let config_overlays = config_dir.join("rustizarr/overlays");
            if config_overlays.exists() {
                return config_overlays;
            }
        }
        
        // 3. Dossier courant (développement)
        let current_dir = PathBuf::from("overlays");
        if current_dir.exists() {
            return current_dir;
        }
        
        // 4. Dossier overlays (depuis la racine du projet)
        let backend_overlays = PathBuf::from("overlays");
        if backend_overlays.exists() {
            return backend_overlays;
        }
        
        // 5. Fallback par défaut
        PathBuf::from("overlays")
    }

    /// Télécharge et standardise une image à 2000x3000px
    pub async fn download_image(url: &str) -> Result<DynamicImage> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        let resp = client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        let img = image::load_from_memory(&bytes)?;

        // Standardisation : Force 2000x3000 pixels pour uniformité
        let standardized_img = img.resize_exact(2000, 3000, imageops::FilterType::Lanczos3);

        Ok(standardized_img)
    }

    /// Applique les gradients haut et bas via PNG overlay
    pub fn add_gradient_masks(mut base_image: DynamicImage, overlays_base: &str) -> anyhow::Result<DynamicImage> {
        let base_path = if overlays_base.is_empty() {
            Self::get_overlays_base_path()
        } else {
            PathBuf::from(overlays_base)
        };
        
        let top_path = base_path.join("gradients/gradient_top.png");
        let bottom_path = base_path.join("gradients/gradient_bottom.png");

        let poster_w = base_image.width();
        let poster_h = base_image.height();

        // 1. Gradient Haut
        if top_path.exists() {
            let top_img = image::open(&top_path)?;
            let scale = poster_w as f32 / top_img.width() as f32;
            let new_h = (top_img.height() as f32 * scale) as u32;
            
            let top_resized = top_img.resize(poster_w, new_h, imageops::FilterType::Lanczos3);
            imageops::overlay(&mut base_image, &top_resized, 0, 0);
        } else {
            println!("   ⚠️ Gradient top introuvable : {:?}", top_path);
        }

        // 2. Gradient Bas
        if bottom_path.exists() {
            let bottom_img = image::open(&bottom_path)?;
            let scale = poster_w as f32 / bottom_img.width() as f32;
            let new_h = (bottom_img.height() as f32 * scale) as u32;
            
            let bottom_resized = bottom_img.resize(poster_w, new_h, imageops::FilterType::Lanczos3);
            let y_pos = poster_h - bottom_resized.height();
            imageops::overlay(&mut base_image, &bottom_resized, 0, y_pos as i64);
        } else {
            println!("   ⚠️ Gradient bottom introuvable : {:?}", bottom_path);
        }
        
        Ok(base_image)
    }

    /// Ajoute le titre du film/série en bas (multiline + word wrap)
    pub fn add_movie_title(base_image: DynamicImage, title: &str, overlays_base: &str) -> anyhow::Result<DynamicImage> {
        let base_path = if overlays_base.is_empty() {
            Self::get_overlays_base_path()
        } else {
            PathBuf::from(overlays_base)
        };
        
        let font_path = base_path.join("fonts/Colus-Regular.ttf");
        if !font_path.exists() { 
            println!("   ⚠️ Police Colus introuvable : {:?}", font_path);
            return Ok(base_image); 
        }
        
        let font_data = fs::read(&font_path)?;
        let font = Font::try_from_vec(font_data)
            .ok_or_else(|| anyhow::anyhow!("Erreur chargement police"))?;

        let mut image_rgba = base_image.to_rgba8();
        let img_width = image_rgba.width() as i32;
        let img_height = image_rgba.height() as i32;
        let title_upper = title.to_uppercase();

        // Taille de police fixe et imposante
        let font_size = 250.0;
        let scale = Scale::uniform(font_size);
        
        // Marge de sécurité (92% de la largeur)
        let max_line_width = img_width as f32 * 0.92;

        // Word wrapping
        let words: Vec<&str> = title_upper.split_whitespace().collect();
        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();

        for word in words {
            let attempt = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };

            let (w, _) = text_size(scale, &font, &attempt);
            
            if w as f32 > max_line_width {
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                current_line = word.to_string();
            } else {
                current_line = attempt;
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // Calcul position verticale (430px du bas)
        let margin_bottom = 430;
        let line_height = font_size * 0.85;
        let total_text_height = lines.len() as f32 * line_height;
        let start_y = (img_height as f32 - margin_bottom as f32 - total_text_height) as i32;

        // Dessin ligne par ligne
        for (i, line) in lines.iter().enumerate() {
            let (w, _h) = text_size(scale, &font, line);
            let x = (img_width - w) / 2;
            let y = start_y + (i as f32 * line_height) as i32;

            // Ombre portée (Noir)
            draw_text_mut(&mut image_rgba, Rgba([0, 0, 0, 220]), x + 5, y + 5, scale, &font, line);
            // Texte principal (Blanc)
            draw_text_mut(&mut image_rgba, Rgba([255, 255, 255, 255]), x, y, scale, &font, line);
        }

        Ok(DynamicImage::ImageRgba8(image_rgba))
    }

    /// Ajoute le border inner glow
    pub fn add_inner_glow_border(mut base_image: DynamicImage, overlays_base: &str) -> anyhow::Result<DynamicImage> {
        let base_path = if overlays_base.is_empty() {
            Self::get_overlays_base_path()
        } else {
            PathBuf::from(overlays_base)
        };
        
        let border_path = base_path.join("overlay-innerglow.png");
        if !border_path.exists() { 
            println!("   ⚠️ Inner glow introuvable : {:?}", border_path);
            return Ok(base_image); 
        }

        let border_img = image::open(&border_path)?;
        let border_resized = border_img.resize_exact(
            base_image.width(),
            base_image.height(),
            imageops::FilterType::Lanczos3
        );
        imageops::overlay(&mut base_image, &border_resized, 0, 0);
        Ok(base_image)
    }
    
    /// Ajoute une bordure (status ou recently added)
    pub fn add_status_border(mut base_image: DynamicImage, overlays_base: &str, status_filename: &str) -> anyhow::Result<DynamicImage> {
        let base_path = if overlays_base.is_empty() {
            Self::get_overlays_base_path()
        } else {
            PathBuf::from(overlays_base)
        };
        
        // Si c'est "recently_added.png", chercher à la racine
        // Sinon chercher dans le dossier Status/
        let border_path = if status_filename == "recently_added.png" {
            base_path.join(status_filename)
        } else {
            base_path.join("Status").join(status_filename)
        };
        
        println!("   🔍 Recherche bordure : {:?}", border_path);
        
        if !border_path.exists() { 
            println!("   ⚠️ Bordure introuvable : {:?}", border_path);
            println!("   🔄 Utilisation de l'inner glow par défaut");
            return Self::add_inner_glow_border(base_image, overlays_base);
        }

        let border_img = image::open(&border_path)?;
        
        let border_resized = border_img.resize_exact(
            base_image.width(),
            base_image.height(),
            imageops::FilterType::Lanczos3
        );
        
        imageops::overlay(&mut base_image, &border_resized, 0, 0);
        println!("   ✅ Bordure '{}' appliquée", status_filename);
        
        Ok(base_image)
    }

    /// Ajoute un overlay (résolution, édition, status, recently added, etc.)
    /// `align_bottom`: true = coin bas-gauche, false = coin haut-gauche
    /// `offset_index`: position dans la pile d'overlays (0, 1, 2...)
    pub fn add_overlay(
        mut base_image: DynamicImage, 
        overlay_path: &Path, 
        offset_index: usize, 
        align_bottom: bool,
        height_percentage: f32 
    ) -> Result<DynamicImage> {
        
        if !overlay_path.exists() { 
            println!("      ⚠️ Overlay introuvable : {:?}", overlay_path);
            return Ok(base_image); 
        }
        
        let overlay = image::open(overlay_path)?;
        let target_icon_height = (base_image.height() as f32 * height_percentage) as u32;

        if target_icon_height == 0 { return Ok(base_image); }

        let scale_factor = target_icon_height as f32 / overlay.height() as f32;
        let target_icon_width = (overlay.width() as f32 * scale_factor) as u32;

        let overlay_resized = overlay.resize_exact(
            target_icon_width, 
            target_icon_height, 
            imageops::FilterType::Lanczos3
        );

        let margin = 30; 
        let spacing = 12;

        let final_x = margin + (offset_index as u32 * (overlay_resized.width() + spacing));
        
        let final_y = if align_bottom {
            base_image.height() - overlay_resized.height() - margin
        } else {
            margin
        };

        imageops::overlay(&mut base_image, &overlay_resized, final_x as i64, final_y as i64);

        Ok(base_image)
    }

    /// Ajoute un badge audience score en bas à droite avec note superposée
    pub fn add_overlay_bottom_right(
        mut base_image: DynamicImage, 
        overlay_path: &Path, 
        height_percentage: f32,
        score: Option<f64>,
        overlays_base: &str
    ) -> Result<DynamicImage> {
        
        if !overlay_path.exists() { 
            println!("      ⚠️ Badge audience introuvable : {:?}", overlay_path);
            return Ok(base_image); 
        }
        
        let overlay = image::open(overlay_path)?;
        
        let target_icon_height = (base_image.height() as f32 * height_percentage) as u32;
        if target_icon_height == 0 { return Ok(base_image); }

        let scale_factor = target_icon_height as f32 / overlay.height() as f32;
        let target_icon_width = (overlay.width() as f32 * scale_factor) as u32;

        let overlay_resized = overlay.resize_exact(
            target_icon_width, 
            target_icon_height, 
            imageops::FilterType::Lanczos3
        );

        let margin = 30; 
        let badge_x = base_image.width() - overlay_resized.width() - margin;
        let badge_y = base_image.height() - overlay_resized.height() - margin;

        imageops::overlay(&mut base_image, &overlay_resized, badge_x as i64, badge_y as i64);

        // Ajout de la note si fournie
        if let Some(val) = score {
            let base_path = if overlays_base.is_empty() {
                Self::get_overlays_base_path()
            } else {
                PathBuf::from(overlays_base)
            };
            
            let font_path = base_path.join("fonts/AvenirNextLTPro-Bold.ttf");
            if font_path.exists() {
                let font_data = fs::read(font_path)?;
                if let Some(font) = Font::try_from_vec(font_data) {
                    let score_text = format!("{:.1}", val);
                    let mut image_rgba = base_image.to_rgba8();
                    let font_size = target_icon_height as f32 * 0.65;
                    let scale = Scale::uniform(font_size);
                    let (text_w, text_h) = text_size(scale, &font, &score_text);
                    
                    let text_x = badge_x as i32 + (overlay_resized.width() as i32 - text_w) / 2;
                    let text_y = badge_y as i32 + (overlay_resized.height() as i32 - text_h) / 2 + 2;

                    // Note en noir pour contraste sur badge clair
                    draw_text_mut(&mut image_rgba, Rgba([0, 0, 0, 255]), text_x, text_y, scale, &font, &score_text);
                    return Ok(DynamicImage::ImageRgba8(image_rgba));
                }
            } else {
                println!("      ⚠️ Police Avenir introuvable pour note : {:?}", font_path);
            }
        }
        
        Ok(base_image)
    }
}
