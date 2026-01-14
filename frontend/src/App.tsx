import { useEffect, useState } from 'react';
import { Activity, Zap, Search, LayoutGrid, List, CheckCircle2, Clock, HardDrive, RefreshCw, Filter, X, ArrowLeftRight } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { Toaster, toast } from 'sonner';

import { Card } from "./components/ui/card";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Input } from "./components/ui/input";

// --- TYPES ---
interface PlexLabel {
  tag: string;
}

interface PlexMedia {
  videoResolution?: string;
  audioCodec?: string;
  Part?: any;
}

interface PlexMovie {
  title: string;
  ratingKey: string;
  year?: number;
  audienceRating?: number;
  Label?: PlexLabel[];
  Media?: PlexMedia[];
}

interface MovieDisplay {
  id: string;
  title: string;
  year: string;
  status: 'processed' | 'pending';
  resolution: string;
  rating?: number;
  audioCodec?: string;
}

type FilterType = 'all' | 'processed' | 'pending';

function App() {
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [movies, setMovies] = useState<MovieDisplay[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const [filterStatus, setFilterStatus] = useState<FilterType>('all');
  const [selectedMovie, setSelectedMovie] = useState<MovieDisplay | null>(null);
  const [comparisonMode, setComparisonMode] = useState<'side-by-side' | 'slider'>('side-by-side');

  useEffect(() => {
    fetchLibrary();
  }, []);

  const fetchLibrary = async () => {
    try {
      const response = await fetch('http://localhost:3000/api/library');
      const plexMovies: PlexMovie[] = await response.json();

      const formattedMovies: MovieDisplay[] = plexMovies.map(m => {
        let labelsArray: any[] = [];
        if (Array.isArray(m.Label)) {
            labelsArray = m.Label;
        } else if (m.Label) {
            labelsArray = [m.Label];
        }

        const isProcessed = labelsArray.some((l: any) => l.tag?.toLowerCase() === 'rustizarr');
        const res = m.Media?.[0]?.videoResolution?.toUpperCase() || "UNK";
        const audio = m.Media?.[0]?.audioCodec?.toUpperCase() || "";

        return {
          id: m.ratingKey,
          title: m.title,
          year: m.year?.toString() || "----",
          status: isProcessed ? 'processed' : 'pending',
          resolution: res,
          rating: m.audienceRating,
          audioCodec: audio
        };
      });

      setMovies(formattedMovies);
      setLoading(false);
    } catch (error) {
      console.error("Erreur de connexion au Backend:", error);
      toast.error("Erreur de connexion au serveur");
      setLoading(false);
    }
  };

  const refreshLibrary = async () => {
    setRefreshing(true);
    const toastId = toast.loading('Rafraîchissement du cache...');
    
    try {
      const response = await fetch('http://localhost:3000/api/library/refresh', {
        method: 'POST'
      });
      const data = await response.json();
      
      if (data.success) {
        await fetchLibrary();
        toast.success(`✅ ${data.total} films rechargés (${data.processed} traités)`, {
          id: toastId,
          duration: 3000
        });
      } else {
        toast.error('Échec du rafraîchissement', { id: toastId });
      }
    } catch (error) {
      console.error('❌ Erreur refresh:', error);
      toast.error('Erreur lors du rafraîchissement', { id: toastId });
    } finally {
      setRefreshing(false);
    }
  };

  const triggerScan = async () => {
    toast.promise(
      fetch('http://localhost:3000/scan'),
      {
        loading: 'Lancement du scan...',
        success: '🚀 Scan lancé ! Surveillez le terminal serveur',
        error: 'Erreur lors du lancement du scan'
      }
    );
  };

  // Filtrage combiné : recherche + statut
  const filteredMovies = movies.filter(m => {
    const matchesSearch = m.title.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesStatus = filterStatus === 'all' || m.status === filterStatus;
    return matchesSearch && matchesStatus;
  });

  const stats = {
    total: movies.length,
    processed: movies.filter(m => m.status === 'processed').length,
    pending: movies.filter(m => m.status === 'pending').length
  };

  return (
    <div className="min-h-screen p-8 max-w-7xl mx-auto font-sans text-zinc-200">
      <Toaster position="top-right" theme="dark" richColors />
      
      {/* Modal Comparaison */}
      <ComparisonModal 
        movie={selectedMovie} 
        onClose={() => setSelectedMovie(null)}
        mode={comparisonMode}
        onModeChange={setComparisonMode}
      />
      
      {/* HEADER */}
      <header className="flex flex-col md:flex-row justify-between items-start md:items-end mb-10 gap-6">
        <div>
          <div className="flex items-center gap-3 mb-2">
            <div className="relative flex h-3 w-3">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-primary opacity-75"></span>
              <span className="relative inline-flex rounded-full h-3 w-3 bg-primary"></span>
            </div>
            <h2 className="text-xs font-semibold tracking-widest text-primary uppercase">System Online</h2>
          </div>
          <h1 className="text-4xl font-bold tracking-tight text-white">
            Rustizarr <span className="text-zinc-600">Dashboard</span>
          </h1>
        </div>

        <div className="flex gap-3">
            <StatsCard label="Films Total" value={stats.total.toString()} icon={<HardDrive size={16} className="text-white"/>} />
            <StatsCard label="Traités" value={stats.processed.toString()} icon={<CheckCircle2 size={16} className="text-success"/>} />
            <StatsCard label="En Attente" value={stats.pending.toString()} icon={<Clock size={16} className="text-warning"/>} />
        </div>
      </header>

      {/* CONTROLS */}
      <div className="space-y-4 mb-8">
        {/* Ligne 1 : Recherche + View Mode */}
        <div className="flex flex-col md:flex-row justify-between items-center gap-4 bg-surface/50 p-2 rounded-xl border border-white/5">
          <div className="relative w-full md:w-96">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 h-4 w-4" />
            <Input 
              placeholder="Rechercher un film..." 
              className="pl-10 bg-back border-zinc-800 focus:border-primary/50 text-sm h-9"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
          </div>
          
          <div className="flex items-center gap-2">
             <div className="flex bg-back rounded-lg p-1 border border-zinc-800">
               <Button 
                variant="ghost" size="icon" className={`h-7 w-7 ${viewMode === 'grid' ? 'bg-zinc-800 text-white' : 'text-zinc-500'}`}
                onClick={() => setViewMode('grid')}
               >
                 <LayoutGrid size={14} />
               </Button>
               <Button 
                variant="ghost" size="icon" className={`h-7 w-7 ${viewMode === 'list' ? 'bg-zinc-800 text-white' : 'text-zinc-500'}`}
                onClick={() => setViewMode('list')}
               >
                 <List size={14} />
               </Button>
             </div>
             
             <div className="w-px h-6 bg-zinc-800 mx-2"></div>
             
             <Button 
               onClick={refreshLibrary} 
               disabled={refreshing}
               variant="outline"
               className="bg-zinc-900 text-zinc-300 hover:bg-zinc-800 border-zinc-800 font-medium text-xs h-9"
             >
               <RefreshCw size={14} className={`mr-2 ${refreshing ? 'animate-spin' : ''}`} />
               {refreshing ? 'Rafraîchissement...' : 'Rafraîchir'}
             </Button>
             
             <Button 
               onClick={triggerScan} 
               className="bg-white text-black hover:bg-zinc-200 font-medium text-xs h-9 shadow-lg shadow-white/5"
             >
               <Zap size={14} className="mr-2 fill-black" /> Lancer Scan
             </Button>
          </div>
        </div>

        {/* Ligne 2 : Filtres par Statut */}
        <div className="flex items-center gap-2 px-2">
          <Filter size={14} className="text-zinc-500" />
          <span className="text-xs text-zinc-500 font-medium uppercase tracking-wider">Filtrer :</span>
          
          <div className="flex gap-2">
            <FilterButton 
              label="Tous" 
              count={stats.total}
              active={filterStatus === 'all'}
              onClick={() => setFilterStatus('all')}
            />
            <FilterButton 
              label="Traités" 
              count={stats.processed}
              active={filterStatus === 'processed'}
              onClick={() => setFilterStatus('processed')}
              color="emerald"
            />
            <FilterButton 
              label="En Attente" 
              count={stats.pending}
              active={filterStatus === 'pending'}
              onClick={() => setFilterStatus('pending')}
              color="amber"
            />
          </div>
          
          {filterStatus !== 'all' && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setFilterStatus('all')}
              className="text-xs text-zinc-500 hover:text-zinc-300 h-7"
            >
              Réinitialiser
            </Button>
          )}
        </div>
      </div>

      {/* CONTENT */}
      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 gap-4">
          <Activity className="animate-spin text-primary" size={32} />
          <p className="text-zinc-500 text-sm">Chargement de la bibliothèque...</p>
        </div>
      ) : filteredMovies.length === 0 ? (
        <div className="text-center py-20 text-zinc-500">
          <p className="text-lg">Aucun film trouvé</p>
          <p className="text-sm mt-2">Essayez de modifier vos filtres</p>
        </div>
      ) : (
        <motion.div 
          layout
          className={`grid gap-4 ${viewMode === 'grid' ? 'grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5' : 'grid-cols-1'}`}
        >
          <AnimatePresence>
            {filteredMovies.map((movie) => (
              <MovieCard 
                key={movie.id} 
                movie={movie} 
                mode={viewMode}
                onClick={() => movie.status === 'processed' && setSelectedMovie(movie)}
              />
            ))}
          </AnimatePresence>
        </motion.div>
      )}

    </div>
  )
}

// --- SOUS-COMPOSANTS ---

const StatsCard = ({ label, value, icon }: any) => (
  <div className="flex items-center gap-3 px-4 py-2 bg-surface border border-white/5 rounded-lg shadow-sm">
    <div className="p-2 bg-white/5 rounded-md">{icon}</div>
    <div>
      <div className="text-xl font-bold leading-none text-zinc-100">{value}</div>
      <div className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider mt-1">{label}</div>
    </div>
  </div>
);

const FilterButton = ({ label, count, active, onClick, color = "zinc" }: any) => {
  const colors = {
    zinc: 'bg-zinc-800 text-zinc-300 border-zinc-700',
    emerald: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20',
    amber: 'bg-amber-500/10 text-amber-400 border-amber-500/20'
  };

  return (
    <Button
      variant="outline"
      size="sm"
      onClick={onClick}
      className={`text-xs h-7 gap-2 transition-all ${
        active ? colors[color as keyof typeof colors] : 'bg-zinc-900 text-zinc-500 border-zinc-800 hover:bg-zinc-800'
      }`}
    >
      {label}
      <Badge variant="secondary" className="h-4 px-1.5 text-[10px] bg-white/10">
        {count}
      </Badge>
    </Button>
  );
};

const MovieCard = ({ movie, mode, onClick }: { movie: MovieDisplay, mode: 'grid' | 'list', onClick?: () => void }) => {
  const [showPreview, setShowPreview] = useState(false);
  const isGrid = mode === 'grid';
  const isProcessed = movie.status === 'processed';
  
  const statusConfig = {
    processed: { color: 'bg-emerald-500/10 text-emerald-500 border-emerald-500/20', dot: 'bg-emerald-500' },
    pending: { color: 'bg-amber-500/10 text-amber-500 border-amber-500/20', dot: 'bg-amber-500' },
  }[movie.status];

  return (
    <motion.div 
      layout
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.9 }}
      transition={{ duration: 0.2 }}
      onMouseEnter={() => setShowPreview(true)}
      onMouseLeave={() => setShowPreview(false)}
      onClick={onClick}
      className={`group relative bg-surface border border-white/5 rounded-xl overflow-hidden hover:border-primary/30 transition-all duration-300 hover:shadow-2xl hover:shadow-black/50
        ${isGrid ? 'flex flex-col' : 'flex items-center p-3 gap-4 h-20'}
        ${isProcessed ? 'cursor-pointer' : ''}`}
    >
      {/* Preview Overlay (Grid uniquement) */}
      {isGrid && showPreview && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="absolute inset-0 bg-black/95 z-10 p-4 flex flex-col justify-center gap-2"
        >
          <h3 className="font-bold text-white text-base">{movie.title}</h3>
          <div className="space-y-1 text-xs text-zinc-400">
            <div className="flex items-center gap-2">
              <span className="text-zinc-500">📅</span>
              <span>{movie.year}</span>
            </div>
            {movie.rating && (
              <div className="flex items-center gap-2">
                <span className="text-yellow-500">⭐</span>
                <span>{movie.rating.toFixed(1)}/10</span>
              </div>
            )}
            <div className="flex items-center gap-2">
              <span className="text-zinc-500">🎬</span>
              <span>{movie.resolution}</span>
            </div>
            {movie.audioCodec && (
              <div className="flex items-center gap-2">
                <span className="text-zinc-500">🔊</span>
                <span>{movie.audioCodec}</span>
              </div>
            )}
          </div>
          <Badge variant="outline" className={`${statusConfig.color} border w-fit mt-2`}>
            {movie.status === 'processed' ? '✓ Traité' : '⏳ En attente'}
          </Badge>
          {isProcessed && (
            <p className="text-xs text-primary mt-2 flex items-center gap-1">
              👁️ Cliquez pour voir le comparatif
            </p>
          )}
        </motion.div>
      )}
   
      <div className={`relative bg-zinc-900 overflow-hidden flex items-center justify-center border-b border-white/5
        ${isGrid ? 'aspect-[2/3] w-full' : 'h-full aspect-[2/3] rounded-md border-b-0'}`}>
        
        <img 
            src={`http://localhost:3000/api/image/${movie.id}`} 
            alt={movie.title}
            className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-110"
            loading="lazy" 
        />
        
        <div className="absolute inset-0 bg-primary/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
      </div>

      <div className={`flex flex-col flex-grow min-w-0 ${isGrid ? 'p-3' : 'py-1 pr-4'}`}>
        <div className="flex justify-between items-start">
          <div className="min-w-0">
            <h3 className="font-medium text-zinc-200 group-hover:text-primary transition-colors text-sm truncate pr-2" title={movie.title}>
                {movie.title}
            </h3>
            <p className="text-[11px] text-zinc-500 mt-0.5 font-mono">{movie.year} • {movie.resolution}</p>
          </div>
          
          {isGrid && (
             <Badge variant="outline" className={`${statusConfig.color} border h-4 px-1.5 text-[9px] uppercase tracking-wide gap-1 rounded-sm`}>
                <span className={`w-1 h-1 rounded-full ${statusConfig.dot}`}></span>
                {movie.status === 'processed' ? 'OK' : 'WAIT'}
             </Badge>
          )}
        </div>
        
        {!isGrid && (
             <div className="ml-auto flex items-center gap-4">
                 <Badge variant="outline" className={`${statusConfig.color} border h-5 px-2 text-[10px] uppercase tracking-wide gap-1.5`}>
                    <span className={`w-1.5 h-1.5 rounded-full ${statusConfig.dot} animate-pulse`}></span>
                    {movie.status}
                 </Badge>
             </div>
        )}

        {isGrid && (
          <div className="mt-auto pt-3 border-t border-white/5 flex items-center justify-between text-[10px] text-zinc-600 font-mono">
            <span>ID: {movie.id}</span>
          </div>
        )}
      </div>
    </motion.div>
  );
};

// --- MODAL DE COMPARAISON ---
const ComparisonModal = ({ 
  movie, 
  onClose, 
  mode, 
  onModeChange 
}: { 
  movie: MovieDisplay | null; 
  onClose: () => void;
  mode: 'side-by-side' | 'slider';
  onModeChange: (mode: 'side-by-side' | 'slider') => void;
}) => {
  const [sliderPosition, setSliderPosition] = useState(50);
  const [isDragging, setIsDragging] = useState(false);

  if (!movie) return null;

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDragging || mode !== 'slider') return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const percentage = (x / rect.width) * 100;
    setSliderPosition(Math.max(0, Math.min(100, percentage)));
  };

  // URL du poster original Plex (sans traitement)
  const originalPosterUrl = `http://localhost:3000/api/image/${movie.id}`;
  
  // Pour l'instant, on utilise la même image car on n'a pas l'original
  // Dans une vraie implémentation, tu pourrais stocker l'URL de l'original
  const processedPosterUrl = `http://localhost:3000/api/image/${movie.id}`;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 bg-black/90 backdrop-blur-sm z-50 flex items-center justify-center p-4"
        onClick={onClose}
      >
        <motion.div
          initial={{ scale: 0.9, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={{ scale: 0.9, opacity: 0 }}
          transition={{ type: "spring", damping: 25 }}
          className="bg-zinc-900 rounded-2xl border border-white/10 max-w-6xl w-full max-h-[90vh] overflow-hidden shadow-2xl"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div className="p-6 border-b border-white/10 flex items-center justify-between">
            <div>
              <h2 className="text-2xl font-bold text-white">{movie.title}</h2>
              <p className="text-sm text-zinc-400 mt-1">
                {movie.year} • {movie.resolution} {movie.rating && `• ⭐ ${movie.rating.toFixed(1)}`}
              </p>
            </div>
            
            <div className="flex items-center gap-3">
              {/* Toggle Mode */}
              <div className="flex bg-zinc-800 rounded-lg p-1 border border-zinc-700">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onModeChange('side-by-side')}
                  className={`text-xs h-8 ${mode === 'side-by-side' ? 'bg-zinc-700 text-white' : 'text-zinc-400'}`}
                >
                  Côte à côte
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onModeChange('slider')}
                  className={`text-xs h-8 ${mode === 'slider' ? 'bg-zinc-700 text-white' : 'text-zinc-400'}`}
                >
                  <ArrowLeftRight size={14} className="mr-1" />
                  Slider
                </Button>
              </div>

              <Button
                variant="ghost"
                size="icon"
                onClick={onClose}
                className="text-zinc-400 hover:text-white"
              >
                <X size={20} />
              </Button>
            </div>
          </div>

          {/* Content */}
          <div className="p-6">
            {mode === 'side-by-side' ? (
              // Mode Côte à Côte
              <div className="grid grid-cols-2 gap-6">
                {/* AVANT */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-zinc-400 uppercase tracking-wider">
                      ⬅️ Original Plex
                    </h3>
                    <Badge variant="outline" className="text-xs border-zinc-700">
                      Sans traitement
                    </Badge>
                  </div>
                  <div className="aspect-[2/3] rounded-xl overflow-hidden border-2 border-zinc-800 shadow-xl">
                    <img 
                      src={originalPosterUrl}
                      alt="Original"
                      className="w-full h-full object-cover"
                    />
                  </div>
                  <div className="text-xs text-zinc-500 space-y-1 bg-zinc-800/50 rounded-lg p-3">
                    <p>• Poster standard Plex</p>
                    <p>• Sans overlays</p>
                    <p>• Sans badges</p>
                  </div>
                </div>

                {/* APRÈS */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-primary uppercase tracking-wider">
                      Rustizarr ➡️
                    </h3>
                    <Badge className="text-xs bg-primary/20 text-primary border-primary/30">
                      ✨ Amélioré
                    </Badge>
                  </div>
                  <div className="aspect-[2/3] rounded-xl overflow-hidden border-2 border-primary/50 shadow-xl shadow-primary/20">
                    <img 
                      src={processedPosterUrl}
                      alt="Rustizarr"
                      className="w-full h-full object-cover"
                    />
                  </div>
                  <div className="text-xs text-zinc-300 space-y-1 bg-primary/10 rounded-lg p-3 border border-primary/20">
                    <p>✓ Gradient masks appliqués</p>
                    <p>✓ Badges résolution/audio</p>
                    <p>✓ Titre stylisé</p>
                    <p>✓ Note TMDB visible</p>
                  </div>
                </div>
              </div>
            ) : (
              // Mode Slider
              <div className="space-y-3">
                <div className="text-center mb-4">
                  <p className="text-sm text-zinc-400">
                    👆 Glissez le curseur pour comparer
                  </p>
                </div>
                
                <div 
                  className="relative aspect-[2/3] max-h-[60vh] mx-auto rounded-xl overflow-hidden border-2 border-zinc-700 shadow-2xl cursor-col-resize"
                  onMouseMove={handleMouseMove}
                  onMouseDown={() => setIsDragging(true)}
                  onMouseUp={() => setIsDragging(false)}
                  onMouseLeave={() => setIsDragging(false)}
                >
                  {/* Image APRÈS (pleine largeur) */}
                  <img 
                    src={processedPosterUrl}
                    alt="Rustizarr"
                    className="absolute inset-0 w-full h-full object-cover"
                  />

                  {/* Image AVANT (clippée) */}
                  <div 
                    className="absolute inset-0 overflow-hidden"
                    style={{ width: `${sliderPosition}%` }}
                  >
                    <img 
                      src={originalPosterUrl}
                      alt="Original"
                      className="w-full h-full object-cover"
                      style={{ width: `${(100 / sliderPosition) * 100}%` }}
                    />
                  </div>

                  {/* Ligne de séparation */}
                  <div 
                    className="absolute top-0 bottom-0 w-1 bg-white shadow-lg"
                    style={{ left: `${sliderPosition}%` }}
                  >
                    {/* Handle */}
                    <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-10 h-10 bg-white rounded-full shadow-xl flex items-center justify-center border-4 border-zinc-900">
                      <ArrowLeftRight size={18} className="text-zinc-900" />
                    </div>
                  </div>

                  {/* Labels */}
                  <div className="absolute top-4 left-4 bg-black/70 backdrop-blur-sm px-3 py-1.5 rounded-lg text-xs font-semibold">
                    ⬅️ Original
                  </div>
                  <div className="absolute top-4 right-4 bg-primary/80 backdrop-blur-sm px-3 py-1.5 rounded-lg text-xs font-semibold text-white">
                    Rustizarr ➡️
                  </div>
                </div>

                <div className="text-center text-xs text-zinc-500">
                  Position : {sliderPosition.toFixed(0)}%
                </div>
              </div>
            )}
          </div>

          {/* Footer */}
          <div className="p-6 border-t border-white/10 flex items-center justify-between bg-zinc-800/50">
            <div className="flex items-center gap-2 text-xs text-zinc-400">
              <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
              Film traité avec succès
            </div>
            
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                className="text-xs border-zinc-700"
                onClick={onClose}
              >
                Fermer
              </Button>
              <Button
                size="sm"
                className="text-xs bg-primary hover:bg-primary/90"
              >
                🔄 Retraiter
              </Button>
            </div>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
};

export default App;