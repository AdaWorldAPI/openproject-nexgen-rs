class Tables::WorkPackages < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.string   :subject, null: false
      t.text     :description
      t.integer  :status_id
      t.string   :status
      t.float    :total_hours
      t.datetime :created_at, null: false
      t.datetime :updated_at, null: false
    end
  end
end
