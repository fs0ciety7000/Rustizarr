use image::{imageops, GenericImageView, DynamicImage, Rgba, RgbaImage, Pixel};
use std::path::Path;
use anyhow::Result;
use imageproc::drawing::{draw_text_mut, text_size};
use rusttype::{Font, Scale};
use std::fs;

pub struct ImageProcessor;

impl ImageProcessor {
    pub async fn download_image(url: &str) -> Result<DynamicImage> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()?;
        let resp = client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        let img = image::load_from_memory(&bytes)?;

        // --- CORRECTIF : STANDARDISATION DE LA TAILLE ---
        // On force toutes les affiches en 2000x3000 pixels.
        // Ainsi, nos calculs de police (190px) et de marges (430px) seront constants.
        let standardized_img = img.resize_exact(2000, 3000, imageops::FilterType::Lanczos3);

        Ok(standardized_img)
    }

    // --- NOUVEAU : UTILISATION DES IMAGES PNG POUR LE GRADIENT ---
    pub fn add_gradient_masks(mut base_image: DynamicImage) -> Result<DynamicImage> {
        let top_path = Path::new("../overlays/gradients/gradient_top.png");
        let bottom_path = Path::new("../overlays/gradients/gradient_bottom.png");

        let poster_w = base_image.width();
        let poster_h = base_image.height();

        // 1. Application du Gradient Haut (S'il existe)
        if top_path.exists() {
            let top_img = image::open(top_path)?;
            // On redimensionne en largeur (poster_w) et on garde le ratio pour la hauteur
            let scale = poster_w as f32 / top_img.width() as f32;
            let new_h = (top_img.height() as f32 * scale) as u32;
            
            let top_resized = top_img.resize(poster_w, new_h, imageops::FilterType::Lanczos3);
            imageops::overlay(&mut base_image, &top_resized, 0, 0);
        }

        // 2. Application du Gradient Bas (S'il existe)
        if bottom_path.exists() {
            let bottom_img = image::open(bottom_path)?;
            let scale = poster_w as f32 / bottom_img.width() as f32;
            let new_h = (bottom_img.height() as f32 * scale) as u32;

            let bottom_resized = bottom_img.resize(poster_w, new_h, imageops::FilterType::Lanczos3);
            
            // On le colle tout en bas
            let y_pos = poster_h - bottom_resized.height();
            imageops::overlay(&mut base_image, &bottom_resized, 0, y_pos as i64);
        }

        Ok(base_image)
    }

    // ... (Dans impl ImageProcessor) ...

    pub fn add_movie_title(base_image: DynamicImage, title: &str) -> Result<DynamicImage> {
        let font_path = Path::new("../overlays/fonts/Colus-Regular.ttf");
        if !font_path.exists() { return Ok(base_image); }

        let font_data = fs::read(font_path)?;
        let font = Font::try_from_vec(font_data).ok_or(anyhow::anyhow!("Erreur police"))?;

        let mut image_rgba = base_image.to_rgba8();
        let img_width = image_rgba.width() as i32;
        let img_height = image_rgba.height() as i32;
        let title_upper = title.to_uppercase();

        // 1. TAILLE FIXE (Plus de réduction dynamique)
        let font_size = 250.0; // Une bonne taille imposante
        let scale = Scale::uniform(font_size);
        
        // Marge de sécurité sur les côtés (92% de la largeur)
        let max_line_width = img_width as f32 * 0.92;

        // 2. LOGIQUE DE DÉCOUPAGE (WORD WRAPPING)
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
                // Si ça dépasse, on valide la ligne précédente et on commence une nouvelle
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                current_line = word.to_string();
            } else {
                // Sinon on continue la ligne
                current_line = attempt;
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // 3. CALCUL DE LA POSITION VERTICALE
        // On veut que le bas du bloc de texte soit à 430px du bas de l'image.
        let margin_bottom = 430;
        
        // Espacement entre les lignes (un peu moins que la taille de police pour être compact)
        let line_height = font_size * 0.85; 
        
        // Hauteur totale du bloc de texte
        let total_text_height = lines.len() as f32 * line_height;

        // Le point de départ Y (Haut du bloc de texte)
        // Calcul : HauteurImage - MargeBas - HauteurTexte
        let start_y = (img_height as f32 - margin_bottom as f32 - total_text_height) as i32;

        // 4. DESSIN LIGNE PAR LIGNE
        for (i, line) in lines.iter().enumerate() {
            let (w, _h) = text_size(scale, &font, line);
            
            // Centrage horizontal
            let x = (img_width - w) / 2;
            
            // Position Y de la ligne actuelle
            let y = start_y + (i as f32 * line_height) as i32;

            // Ombre portée (Noir)
            draw_text_mut(&mut image_rgba, Rgba([0, 0, 0, 220]), x + 5, y + 5, scale, &font, line);
            // Texte (Blanc)
            draw_text_mut(&mut image_rgba, Rgba([255, 255, 255, 255]), x, y, scale, &font, line);
        }

        Ok(DynamicImage::ImageRgba8(image_rgba))
    }

    pub fn add_inner_glow_border(mut base_image: DynamicImage) -> Result<DynamicImage> {
        let border_path = Path::new("../overlays/overlay-innerglow.png");
        if !border_path.exists() { return Ok(base_image); }

        let border_img = image::open(border_path)?;
        let border_resized = border_img.resize_exact(
            base_image.width(), 
            base_image.height(), 
            imageops::FilterType::Lanczos3
        );
        imageops::overlay(&mut base_image, &border_resized, 0, 0);
        Ok(base_image)
    }

    pub fn add_overlay(
        mut base_image: DynamicImage, 
        overlay_path: &Path, 
        offset_index: usize, 
        align_bottom: bool,
        height_percentage: f32 
    ) -> Result<DynamicImage> {
        
        if !overlay_path.exists() { 
            // AJOUT LOG DEBUG : Pour savoir pourquoi un badge ne s'affiche pas
            println!("      ⚠️ Image Overlay introuvable : {:?}", overlay_path);
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

    pub fn add_overlay_bottom_right(
        mut base_image: DynamicImage, 
        overlay_path: &Path, 
        height_percentage: f32,
        score: Option<f64>
    ) -> Result<DynamicImage> {
        
        if !overlay_path.exists() { return Ok(base_image); }
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

        if let Some(val) = score {
            let font_path = Path::new("../overlays/fonts/AvenirNextLTPro-Bold.ttf");
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

                    draw_text_mut(&mut image_rgba, Rgba([0, 0, 0, 255]), text_x, text_y, scale, &font, &score_text);
                    return Ok(DynamicImage::ImageRgba8(image_rgba));
                }
            }
        }
        Ok(base_image)
    }
}